//! The F7 notes editor (agent notes spec G4, §8): the explorer's diary,
//! memory and proposals, read and written as the player. Present only when
//! the app inserted [`ExplorerFiles`] (explorer mode).
//!
//! - **Tabs** Diary, Memory, Proposals (F7 toggles the panel, Tab cycles the
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
//! - **Input.** While one of the panel's text fields has focus
//!   ([`editor_typing`]), the game's input ([`ViewSystems::Input`]: camera,
//!   orders, panel toggles, the app's speed keys) does not run, so typing
//!   never triggers a game key; Tab moves between the two fields and Escape
//!   leaves them. While the pointer is over the panel, its clicks and wheel
//!   do not reach the world behind it. Copy, cut and paste use the OS
//!   clipboard ([`crate::field_clipboard`]).
//!
//! Notes are host-side files, never sim state; the panel writes nothing
//! else. The default font (Fira Mono subset) has printable ASCII only.

use std::path::PathBuf;
use std::time::Duration;

use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::input_focus::{AutoFocus, FocusCause, InputFocus};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::text::{EditableText, TextCursorStyle};
use bevy::ui::Pressed;
use bevy::ui_widgets::{
    Activate, Button as WidgetButton, ButtonPlugin, ScrollArea, ScrollAreaPlugin, TextInput,
};
use npc_framework::agent_io::notes::{
    Book, NewNote, Note, NoteChange, NotesClock, NotesError, NotesStores, ProposalStatus, Stamp,
    TITLE_MAX, Unreadable, Writer,
};
use npc_framework::prelude::SimClock;
use npc_framework::view_helpers::ViewSystems;

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
    .configure_sets(Update, ViewSystems::Input.run_if(not(editor_typing)))
    .add_systems(Startup, spawn.run_if(resource_exists::<ExplorerFiles>))
    .add_systems(
        Update,
        (
            keys.in_set(ViewSystems::Input).before(apply_requests),
            (field_tab.run_if(editor_typing), shield_world).before(ViewSystems::Input),
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

/// Run condition: one of the panel's text fields has the input focus.
pub fn editor_typing(focus: Option<Res<InputFocus>>, fields: Query<(), With<NoteField>>) -> bool {
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
        ..default()
    }
}

/// A labelled panel button doing `action`.
fn button(parent: &mut ChildSpawnerCommands, label: &str, action: PanelAction) {
    parent
        .spawn((
            action,
            button_node(),
            BackgroundColor(BUTTON),
            WidgetButton,
            Hovered::default(),
        ))
        .with_child((Text::new(label), font(12.0), TextColor(TEXT)));
}

fn spawn(mut commands: Commands) {
    commands
        .spawn((
            Name::new("explorer notes panel"),
            ExplorerPanel,
            Hovered::default(),
            BackgroundColor(PANEL),
            Node {
                position_type: PositionType::Absolute,
                top: px(10),
                right: px(10),
                width: px(560),
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
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((Text::new("F7 NOTES"), font(13.0), TextColor(NOTE)));
                    for book in BOOKS {
                        button(row, book_label(book), PanelAction::Book(book));
                    }
                    row.spawn((Text::new("Tab: next"), font(11.0), TextColor(DIM)));
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
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(4),
                    flex_shrink: 0.0,
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

/// F7 toggles the panel; Tab shows the next book (game input: not while a
/// field has focus).
fn keys(
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mut panels: ResMut<TestbedPanels>,
    mut editor: ResMut<NotesEditor>,
) {
    let Some(keys) = keys else { return };
    if keys.just_pressed(KeyCode::F7) {
        panels.explorer = !panels.explorer;
    }
    if panels.explorer && keys.just_pressed(KeyCode::Tab) {
        let i = BOOKS.iter().position(|b| *b == editor.book).unwrap_or(0);
        let next = BOOKS[(i + 1) % BOOKS.len()];
        editor.requests.push(PanelAction::Book(next));
    }
}

/// Tab in a field moves to the other field.
fn field_tab(
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mut focus: ResMut<InputFocus>,
    title: Query<Entity, With<TitleField>>,
    body: Query<Entity, With<BodyField>>,
) {
    if !keys.is_some_and(|k| k.just_pressed(KeyCode::Tab)) {
        return;
    }
    let (Ok(title), Ok(body)) = (title.single(), body.single()) else {
        return;
    };
    let next = if focus.get() == Some(title) {
        body
    } else {
        title
    };
    focus.set(next, FocusCause::Navigated);
}

/// While the pointer is over the open panel, its clicks and wheel do not
/// reach the world (the command UI, the camera) behind it.
fn shield_world(
    panels: Res<TestbedPanels>,
    roots: Query<&Hovered, With<ExplorerPanel>>,
    mouse: Option<ResMut<ButtonInput<MouseButton>>>,
    scroll: Option<ResMut<AccumulatedMouseScroll>>,
) {
    if !panels.explorer || !roots.iter().any(Hovered::get) {
        return;
    }
    if let Some(mut mouse) = mouse {
        mouse.clear();
    }
    if let Some(mut scroll) = scroll
        && scroll.delta != Vec2::ZERO
    {
        scroll.delta = Vec2::ZERO;
    }
}

/// Reads the book shown; marks the list for a rebuild when it changed.
fn reload(editor: &mut NotesEditor, stores: &mut NotesStores, files: &ExplorerFiles) {
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

fn field(text: EditableText, layout: TextLayout) -> impl Bundle {
    (
        NoteField,
        TextInput,
        text,
        layout,
        font(13.0),
        TextColor(TEXT),
        TextCursorStyle::default(),
        Node {
            width: percent(100),
            min_width: px(0),
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
        match request {
            PanelAction::Book(b) if editor.form.is_some() && b != book => {
                editor.message = Some("save or cancel the entry first".into());
            }
            PanelAction::Book(b) => {
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
) {
    // Visibility follows the flag here (not in `keys`), because input is
    // gated while the prompt modal is open and `--panels explorer` must
    // still show the panel then.
    let current = editor.form.as_ref().and_then(|f| f.current.clone());
    for (mut node, is_root, is_current) in &mut boxes {
        let shown = if is_root {
            panels.explorer
        } else if is_current {
            current.is_some()
        } else {
            editor.form.is_some()
        };
        let want = if shown { Display::Flex } else { Display::None };
        if node.display != want {
            node.display = want;
        }
    }
    let opened = panels.explorer && !state.was_open;
    state.was_open = panels.explorer;
    if !panels.explorer {
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
            PanelAction::Select(note.id),
            Node {
                padding: UiRect::axes(px(3), px(1)),
                border_radius: BorderRadius::all(px(3)),
                ..default()
            },
            BackgroundColor(Color::NONE),
            WidgetButton,
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
    editor: Res<NotesEditor>,
    mut buttons: Query<(
        &PanelAction,
        &mut BackgroundColor,
        Option<&Hovered>,
        Has<Pressed>,
    )>,
) {
    for (action, mut bg, hovered, pressed) in &mut buttons {
        let (active, plain) = match *action {
            PanelAction::Book(b) => (b == editor.book, BUTTON),
            PanelAction::Select(id) => (editor.selected == Some(id), Color::NONE),
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
            .add_plugins(plugin);
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
        app.world_mut().resource_mut::<InputFocus>().clear();
        app.update();
        assert!(app.world().resource::<InputRuns>().0 > runs);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tab_cycles_the_books_and_f7_toggles() {
        let dir = tmp("tab");
        let mut app = app(&dir);
        let press = |app: &mut App, key| {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.release_all();
            keys.clear();
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
