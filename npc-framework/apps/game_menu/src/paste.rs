//! OS copy, cut and paste for the menu's text fields (Ruling R10).
//!
//! The menu's Bevy has no `system_clipboard` feature (its `clipboard-win`
//! dependency is BSL-1.0, which `deny.toml` does not allow), so `TextInput`'s
//! Ctrl+C / Ctrl+X / Ctrl+V (Cmd on macOS, and the Copy / Cut / Paste keys)
//! queue [`TextEdit::Copy`] / [`TextEdit::Cut`] / [`TextEdit::Paste`], which
//! would use Bevy's in-app buffer. [`FieldClipboardPlugin`] splits the queue at
//! each clipboard operation, lets Bevy apply preceding edits, and then uses
//! the OS clipboard instead, through the shared
//! `testbed_content::explorer::clipboard` (the platform's tools, at most 5 s
//! each) on a background thread, never on the frame:
//!
//! - **Copy / Cut** write the selection with `clipboard::copy`. Cut's edit
//!   becomes a [`TextEdit::Delete`] of the selection, so the text leaves the
//!   field after preceding selection edits; a copy that fails is logged.
//! - **Paste** reads `clipboard::paste` and inserts the text at the caret
//!   (replacing a selection) only if text, selection, composition and focus have
//!   remained unchanged. Single-line fields get the first non-blank line only.

use std::sync::{Arc, Mutex};

use bevy::ecs::change_detection::Tick;
use bevy::input::keyboard::KeyboardFocusLost;
use bevy::input_focus::{InputFocus, InputFocusSystems};
use bevy::prelude::*;
use bevy::text::{EditableText, FontCx, LayoutCx, TextEdit, apply_text_edits};
use testbed_content::explorer::clipboard;

/// The text a paste inserts (shared with the F7 notes editor).
pub use clipboard::paste_text;

/// Takes [`TextEdit::Copy`] and [`TextEdit::Cut`] out of a field's queued
/// `edits` and returns the text to put on the OS clipboard: `selected`, the
/// field's selection, when one of them was queued. The first Cut becomes a
/// [`TextEdit::Delete`] in its place when there is a selection (it deletes
/// the selection, not a character); without a selection nothing is copied
/// and nothing deleted.
pub fn take_copy_edits(edits: &mut Vec<TextEdit>, selected: Option<&str>) -> Option<String> {
    if !edits
        .iter()
        .any(|e| matches!(e, TextEdit::Copy | TextEdit::Cut))
    {
        return None;
    }
    let mut cut = false;
    edits.retain_mut(|e| match e {
        TextEdit::Copy => false,
        TextEdit::Cut if selected.is_some() && !cut => {
            *e = TextEdit::Delete;
            cut = true;
            true
        }
        TextEdit::Cut => false,
        _ => true,
    });
    selected.map(str::to_owned)
}

/// The OS clipboard (a resource so tests can stand in for the OS).
#[derive(Resource, Clone, Copy)]
pub struct ClipboardTools {
    /// Reads the clipboard ([`clipboard::paste`]).
    pub paste: fn() -> Result<String, String>,
    /// Writes the clipboard ([`clipboard::copy`]).
    pub copy: fn(&str) -> Result<(), String>,
}

impl Default for ClipboardTools {
    fn default() -> Self {
        Self {
            paste: clipboard::paste,
            copy: clipboard::copy,
        }
    }
}

/// Where a background thread leaves its result.
type Slot<T> = Arc<Mutex<Option<T>>>;

/// Runs `work` on a new thread; its result lands in the returned slot.
fn on_thread<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Slot<T> {
    let slot = Arc::new(Mutex::new(None));
    let out = Arc::clone(&slot);
    std::thread::spawn(move || {
        let result = work();
        if let Ok(mut s) = out.lock() {
            *s = Some(result);
        }
    });
    slot
}

/// The finished result in `slot`, taken out.
fn take<T>(slot: &Slot<T>) -> Option<T> {
    slot.lock().ok().and_then(|mut s| s.take())
}

/// A clipboard operation in flight.
enum Job {
    /// A read for `field`.
    Paste {
        field: Entity,
        result: Option<Slot<Result<String, String>>>,
        unchanged: Box<dyn Fn(&EditableText) -> bool + Send + Sync>,
        focus_epoch: u64,
        focus_tick: Tick,
    },
    /// A write of a copied or cut selection.
    Copy {
        text: String,
        result: Option<Slot<Result<(), String>>>,
    },
}

/// The clipboard operations in flight (the power settings keep the menu
/// awake while there are any).
#[derive(Resource, Default)]
pub struct ClipboardJobs(Vec<Job>, bool, u64);

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ClipboardSystems {
    BeforeEdits,
    Complete,
}

struct Prepared {
    field: Entity,
    operation: TextEdit,
    tail: Vec<TextEdit>,
}
#[derive(Resource, Default)]
struct PreparedClipboard(Vec<Prepared>);
#[derive(Resource, Default)]
struct FocusEpoch(u64);

impl ClipboardJobs {
    /// Nothing in flight.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty() && !self.1
    }
    pub(crate) fn failure_revision(&self) -> u64 {
        self.2
    }
}

/// OS copy, cut and paste for every [`EditableText`] (see the module docs).
pub struct FieldClipboardPlugin;

impl Plugin for FieldClipboardPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ClipboardTools>()
            .init_resource::<ClipboardJobs>()
            .init_resource::<PreparedClipboard>()
            .init_resource::<FocusEpoch>()
            .init_resource::<InputFocus>()
            .init_resource::<FontCx>()
            .init_resource::<LayoutCx>()
            .init_resource::<bevy::clipboard::Clipboard>()
            .add_message::<KeyboardFocusLost>()
            .add_systems(
                PostUpdate,
                finish_jobs
                    .in_set(ClipboardSystems::BeforeEdits)
                    .before(prepare_clipboard),
            )
            .add_systems(PostUpdate, prepare_clipboard.before(apply_text_edits))
            .add_systems(
                PostUpdate,
                complete_clipboard
                    .after(apply_text_edits)
                    .after(bevy::ui::widget::update_editable_text_layout)
                    .after(InputFocusSystems::FocusChangeEvents)
                    .in_set(ClipboardSystems::Complete),
            );
        // Standalone consumers still use the same public Bevy edit application,
        // including character filters; the ordinary menu already has TextPlugin.
        if !app.is_plugin_added::<bevy::text::TextPlugin>() {
            app.add_systems(PostUpdate, apply_text_edits);
        }
        crate::menu_input::install(app);
    }
}

/// Takes Copy, Cut and Paste out of each field's queue (queued in
/// `PreUpdate` by `TextInput`, applied in `PostUpdate`) and starts the OS
/// write or read for them.
fn prepare_clipboard(
    mut fields: Query<(Entity, &mut EditableText)>,
    mut prepared: ResMut<PreparedClipboard>,
    mut jobs: ResMut<ClipboardJobs>,
) {
    jobs.1 = false;
    for (field, mut text) in &mut fields {
        let Some(index) = text
            .pending_edits
            .iter()
            .position(|edit| matches!(edit, TextEdit::Copy | TextEdit::Cut | TextEdit::Paste))
        else {
            continue;
        };
        let mut tail = text.pending_edits.split_off(index);
        let operation = tail.remove(0);
        prepared.0.push(Prepared {
            field,
            operation,
            tail,
        });
    }
}

fn complete_clipboard(
    mut prepared: ResMut<PreparedClipboard>,
    mut fields: Query<&mut EditableText>,
    mut jobs: ResMut<ClipboardJobs>,
    focus: Res<InputFocus>,
    epoch: Res<FocusEpoch>,
    (mut fonts, mut layout, mut clipboard): (
        ResMut<FontCx>,
        ResMut<LayoutCx>,
        ResMut<bevy::clipboard::Clipboard>,
    ),
) {
    for request in prepared.0.drain(..) {
        let Ok(mut text) = fields.get_mut(request.field) else {
            continue;
        };
        let later = std::mem::take(&mut text.pending_edits);
        match request.operation {
            TextEdit::Copy | TextEdit::Cut => {
                let selected = text.editor.selected_text().map(str::to_owned);
                let mut edits = vec![request.operation];
                if let Some(copied) = take_copy_edits(&mut edits, selected.as_deref()) {
                    jobs.0.push(Job::Copy {
                        text: copied,
                        result: None,
                    });
                }
                // The only remaining edit can be Delete, so character filters are
                // unaffected. Apply it before any ordinary edits following Cut.
                text.pending_edits = edits;
                text.apply_pending_edits(&mut fonts, &mut layout.0, &mut clipboard, |_| true);
            }
            TextEdit::Paste => {
                if request.tail.is_empty()
                    && later.is_empty()
                    && focus.get() == Some(request.field)
                    && !text.is_composing()
                {
                    let value = text.value().to_string();
                    let selection = text.editor.raw_selection().text_range();
                    let generation = text.editor.generation();
                    let unchanged = Box::new(move |text: &EditableText| {
                        !text.is_composing()
                            && text.editor.generation() == generation
                            && text.value().to_string() == value
                            && text.editor.raw_selection().text_range() == selection
                    });
                    jobs.0.push(Job::Paste {
                        field: request.field,
                        result: None,
                        unchanged,
                        focus_epoch: epoch.0,
                        // Focus notifications run after UI layout. The resource
                        // tick already records every earlier set/clear, including
                        // away-and-back changes, without ordering I/O after them.
                        focus_tick: focus.last_changed(),
                    });
                } else {
                    jobs.2 = jobs.2.wrapping_add(1);
                }
            }
            _ => unreachable!("only clipboard edits are prepared"),
        }
        text.pending_edits.extend(request.tail);
        text.pending_edits.extend(later);
        jobs.1 |= !text.pending_edits.is_empty();
    }
}

/// Inserts each finished read at its field's caret (a field that is gone,
/// e.g. after a screen change, drops it) and logs a failed read or write.
fn finish_jobs(
    mut jobs: ResMut<ClipboardJobs>,
    mut fields: Query<&mut EditableText>,
    focus: Res<InputFocus>,
    mut epoch: ResMut<FocusEpoch>,
    mut lost: MessageReader<KeyboardFocusLost>,
    tools: Res<ClipboardTools>,
) {
    let jobs = jobs.as_mut();
    if lost.read().count() > 0 {
        epoch.0 = epoch.0.wrapping_add(1);
    }
    // Clipboard reads and writes share the OS clipboard. Run them in order so
    // a fast Copy -> Paste cannot read the old value while its write is pending.
    while let Some(job) = jobs.0.first_mut() {
        let finished = match job {
            Job::Paste {
                field,
                result,
                unchanged,
                focus_epoch,
                focus_tick,
            } => {
                if result.is_none() {
                    let valid = fields.get(*field).is_ok_and(|text| {
                        *focus_epoch == epoch.0
                            && *focus_tick == focus.last_changed()
                            && focus.get() == Some(*field)
                            && text.pending_edits.is_empty()
                            && unchanged(text)
                    });
                    if !valid {
                        jobs.2 = jobs.2.wrapping_add(1);
                        jobs.0.remove(0);
                        continue;
                    }
                    *result = Some(on_thread(tools.paste));
                    break;
                }
                let Some(result) = take(result.as_ref().expect("started read")) else {
                    break;
                };
                match result {
                    Ok(raw) => {
                        if let Ok(mut text) = fields.get_mut(*field) {
                            if *focus_epoch != epoch.0
                                || *focus_tick != focus.last_changed()
                                || focus.get() != Some(*field)
                                || !text.pending_edits.is_empty()
                                || !unchanged(&text)
                            {
                                warn!("game_menu: paste canceled because focus or text changed");
                                jobs.2 = jobs.2.wrapping_add(1);
                                jobs.0.remove(0);
                                continue;
                            }
                            let insert = paste_text(&raw, !text.allow_newlines);
                            if !insert.is_empty() {
                                text.queue_edit(TextEdit::Insert(insert.into()));
                            }
                        }
                    }
                    Err(e) => {
                        jobs.2 = jobs.2.wrapping_add(1);
                        warn!("game_menu: cannot read the clipboard: {e}");
                    }
                }
                true
            }
            Job::Copy { text, result } => {
                if result.is_none() {
                    let copy = tools.copy;
                    let text = text.clone();
                    *result = Some(on_thread(move || copy(&text)));
                    break;
                }
                let Some(result) = take(result.as_ref().expect("started write")) else {
                    break;
                };
                if let Err(e) = result {
                    jobs.2 = jobs.2.wrapping_add(1);
                    warn!("game_menu: cannot write the clipboard: {e}");
                }
                true
            }
        };
        if finished {
            jobs.0.remove(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::text::{DEFAULT_FONT_DATA, Font, FontCx, GenericFontFamily, LayoutCx};
    use bevy::window::PrimaryWindow;

    use super::*;

    #[test]
    fn real_text_ui_focus_graph_accepts_clipboard_order_and_rejects_the_old_reverse_edge() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(bevy::asset::AssetPlugin::default())
            .add_plugins((
                bevy::input::InputPlugin,
                bevy::input_focus::InputFocusPlugin,
                bevy::input_focus::InputDispatchPlugin,
                bevy::transform::TransformPlugin,
                bevy::image::ImagePlugin::default(),
                bevy::mesh::MeshPlugin,
                bevy::camera::CameraPlugin,
                bevy::sprite::SpritePlugin,
                bevy::text::TextPlugin,
                bevy::ui::UiPlugin,
            ))
            .add_plugins(bevy::ui_widgets::UiWidgetsPlugins)
            .add_plugins(bevy::picking::DefaultPickingPlugins)
            .add_plugins(FieldClipboardPlugin);
        assert!(
            app.get_sub_app(bevy::render::RenderApp).is_none(),
            "this schedule test must not initialize a GPU renderer"
        );
        app.world_mut()
            .schedule_scope(PostUpdate, |world, schedule| {
                schedule
                    .initialize(world)
                    .expect("real UI/text/focus ordering must initialize");
            });
        // A negative control proves that this fixture includes the production
        // layout -> focus edge missed by a MinimalPlugins-only text test.
        app.configure_sets(
            PostUpdate,
            ClipboardSystems::BeforeEdits.after(InputFocusSystems::FocusChangeEvents),
        );
        let rejected = app
            .world_mut()
            .schedule_scope(PostUpdate, |world, schedule| {
                schedule.initialize(world).is_err()
            });
        assert!(
            rejected,
            "putting pre-edit clipboard work after late focus processing must form the original dependency cycle"
        );
    }

    #[test]
    fn single_line_fields_take_the_first_line() {
        // `Get-Clipboard` ends every line with CR LF.
        assert_eq!(paste_text("C:\\agents\r\n", true), "C:\\agents");
        assert_eq!(
            paste_text("Wren Ashdown\r\nsecond\r\n", true),
            "Wren Ashdown"
        );
        assert_eq!(paste_text("Hollowmere\nsecond", true), "Hollowmere");
        // Blank lines before the text are skipped; spaces inside are kept.
        assert_eq!(
            paste_text("\r\n  \r\n Brin  Marlow\r\n", true),
            " Brin  Marlow"
        );
        assert_eq!(paste_text("plain", true), "plain");
        assert_eq!(paste_text("", true), "");
        assert_eq!(paste_text("\r\n\r\n", true), "");
    }

    #[test]
    fn multi_line_fields_keep_lines_without_cr() {
        assert_eq!(paste_text("a\r\nb\r\n", false), "a\nb");
        assert_eq!(paste_text("a\nb", false), "a\nb");
    }

    #[test]
    fn copy_and_cut_take_the_selection() {
        // Copy: taken out, the selection is copied.
        let mut edits = vec![TextEdit::Copy];
        assert_eq!(
            take_copy_edits(&mut edits, Some("Wren")),
            Some("Wren".into())
        );
        assert_eq!(edits, vec![]);
        // Cut: the selection is copied and deleted, in the Cut's place.
        let mut edits = vec![TextEdit::Left(false), TextEdit::Cut, TextEdit::Right(false)];
        assert_eq!(take_copy_edits(&mut edits, Some("ash")), Some("ash".into()));
        assert_eq!(
            edits,
            vec![
                TextEdit::Left(false),
                TextEdit::Delete,
                TextEdit::Right(false)
            ]
        );
        // No selection: nothing copied, a Cut deletes nothing.
        let mut edits = vec![TextEdit::Cut, TextEdit::Copy];
        assert_eq!(take_copy_edits(&mut edits, None), None);
        assert_eq!(edits, vec![]);
        // A second Cut has nothing left to cut.
        let mut edits = vec![TextEdit::Cut, TextEdit::Cut];
        assert_eq!(take_copy_edits(&mut edits, Some("x")), Some("x".into()));
        assert_eq!(edits, vec![TextEdit::Delete]);
        // Other edits are left alone.
        let mut edits = vec![TextEdit::Paste, TextEdit::Backspace];
        assert_eq!(take_copy_edits(&mut edits, Some("x")), None);
        assert_eq!(edits, vec![TextEdit::Paste, TextEdit::Backspace]);
    }

    fn fake_paste() -> Result<String, String> {
        Ok("Hollowmere\r\nsecond line\r\n".to_owned())
    }

    /// What [`fake_copy`] was given.
    static COPIED: Mutex<Vec<String>> = Mutex::new(Vec::new());
    static SERIAL: Mutex<()> = Mutex::new(());

    fn fake_copy(text: &str) -> Result<(), String> {
        COPIED.lock().unwrap().push(text.to_owned());
        Ok(())
    }

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(bevy::input_focus::InputFocusPlugin)
            .insert_resource(ClipboardTools {
                paste: fake_paste,
                copy: fake_copy,
            })
            .add_plugins(FieldClipboardPlugin);
        app.world_mut().spawn((Window::default(), PrimaryWindow));
        let font = Font::from_bytes(DEFAULT_FONT_DATA.to_vec());
        let mut fonts = app.world_mut().resource_mut::<FontCx>();
        let ids: Vec<_> = fonts
            .collection
            .register_fonts(font.data, None)
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        for generic in [
            GenericFontFamily::SansSerif,
            GenericFontFamily::Serif,
            GenericFontFamily::Monospace,
            GenericFontFamily::SystemUi,
        ] {
            fonts
                .collection
                .set_generic_families(generic.into(), ids.iter().copied());
        }
        app
    }

    /// Updates until the clipboard jobs are done.
    fn settle(app: &mut App) {
        for _ in 0..400 {
            app.update();
            let pending = app
                .world_mut()
                .query::<&EditableText>()
                .iter(app.world())
                .any(|text| !text.pending_edits.is_empty());
            if app.world().resource::<ClipboardJobs>().is_empty() && !pending {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("the clipboard jobs never finished");
    }

    #[test]
    fn os_paste_replaces_the_in_app_paste() {
        let mut app = app();
        let mut field = EditableText::new("ab");
        // What `TextInput` queues for Ctrl+V.
        field.queue_edit(TextEdit::Paste);
        let id = app.world_mut().spawn(field).id();
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(id, bevy::input_focus::FocusCause::Navigated);
        settle(&mut app);
        let field = app.world().get::<EditableText>(id).unwrap();
        assert_eq!(field.value().to_string(), "abHollowmere");
        assert!(field.pending_edits.is_empty());
    }

    #[test]
    fn ctrl_c_and_ctrl_x_write_the_selection_to_the_os_clipboard() {
        let _serial = SERIAL.lock().unwrap();
        COPIED.lock().unwrap().clear();
        let mut app = app();
        let mut field = EditableText::new("Wren Ashdown");
        field.pending_edits.clear();
        // A selection, as the field's earlier edits leave it (laid out with
        // Bevy's embedded font standing in for every generic family).
        app.world_mut()
            .resource_scope(|world, mut fonts: Mut<FontCx>| {
                let mut layout = world.resource_mut::<LayoutCx>();
                field
                    .editor
                    .driver(&mut fonts, &mut layout.0)
                    .select_byte_range(5, 12);
            });
        assert_eq!(field.editor.selected_text(), Some("Ashdown"));
        // What `TextInput` queues for Ctrl+C.
        field.queue_edit(TextEdit::Copy);
        let id = app.world_mut().spawn(field).id();
        settle(&mut app);
        assert_eq!(*COPIED.lock().unwrap(), vec!["Ashdown".to_owned()]);
        let text = app.world().get::<EditableText>(id).unwrap();
        assert!(text.pending_edits.is_empty(), "{:?}", text.pending_edits);
        // Ctrl+X: copied again, and the in-app Cut became a Delete of the
        // selection (applied by Bevy in `PostUpdate`).
        app.world_mut()
            .get_mut::<EditableText>(id)
            .unwrap()
            .queue_edit(TextEdit::Cut);
        settle(&mut app);
        assert_eq!(COPIED.lock().unwrap().len(), 2);
        let text = app.world().get::<EditableText>(id).unwrap();
        assert_eq!(text.value().to_string(), "Wren ");
        assert!(text.pending_edits.is_empty());
    }

    #[test]
    fn select_all_copy_cut_and_following_insert_keep_edit_order_and_filters() {
        let _serial = SERIAL.lock().unwrap();
        COPIED.lock().unwrap().clear();
        let mut app = app();
        let mut field = EditableText::new("123");
        field.queue_edit(TextEdit::SelectAll);
        field.queue_edit(TextEdit::Insert("invalid".into()));
        field.queue_edit(TextEdit::Copy);
        field.queue_edit(TextEdit::Cut);
        field.queue_edit(TextEdit::Insert("456".into()));
        let id = app
            .world_mut()
            .spawn((
                field,
                bevy::text::EditableTextFilter::new(|ch| ch.is_ascii_digit()),
            ))
            .id();
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(id, bevy::input_focus::FocusCause::Navigated);
        settle(&mut app);
        assert_eq!(*COPIED.lock().unwrap(), ["123", "123"]);
        let field = app.world().get::<EditableText>(id).unwrap();
        assert_eq!(field.value().to_string(), "456");
        assert!(field.pending_edits.is_empty());
    }

    #[test]
    fn same_frame_select_all_then_os_paste_replaces_the_old_field() {
        let mut app = app();
        let mut field = EditableText::new("old-folder");
        field.queue_edit(TextEdit::SelectAll);
        field.queue_edit(TextEdit::Paste);
        let id = app.world_mut().spawn(field).id();
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(id, bevy::input_focus::FocusCause::Navigated);
        settle(&mut app);
        assert_eq!(
            app.world()
                .get::<EditableText>(id)
                .unwrap()
                .value()
                .to_string(),
            "Hollowmere"
        );
    }

    #[test]
    fn os_paste_uses_the_standard_character_filter() {
        let mut app = app();
        let mut field = EditableText::new("123");
        field.queue_edit(TextEdit::SelectAll);
        field.queue_edit(TextEdit::Paste);
        let id = app
            .world_mut()
            .spawn((
                field,
                bevy::text::EditableTextFilter::new(|ch| ch.is_ascii_digit()),
            ))
            .id();
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(id, bevy::input_focus::FocusCause::Navigated);
        settle(&mut app);
        assert_eq!(
            app.world()
                .get::<EditableText>(id)
                .unwrap()
                .value()
                .to_string(),
            "123"
        );
    }

    static PASTE_STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    static PASTE_RELEASE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    struct ReleasePaste;
    impl Drop for ReleasePaste {
        fn drop(&mut self) {
            PASTE_RELEASE.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }
    fn delayed_paste() -> Result<String, String> {
        use std::sync::atomic::Ordering;
        PASTE_STARTED.store(true, Ordering::SeqCst);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while !PASTE_RELEASE.load(Ordering::SeqCst) {
            if std::time::Instant::now() >= deadline {
                return Err("owned clipboard fixture timed out".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        Ok("stale-paste-must-not-arrive".into())
    }
    #[test]
    fn delayed_paste_is_dropped_after_text_caret_ime_focus_or_entity_changes() {
        let _serial = SERIAL.lock().unwrap();
        use std::sync::atomic::Ordering;
        for change in ["text", "caret", "ime", "focus", "despawn"] {
            PASTE_STARTED.store(false, Ordering::SeqCst);
            PASTE_RELEASE.store(false, Ordering::SeqCst);
            let _release = ReleasePaste;
            let mut app = app();
            app.world_mut().resource_mut::<ClipboardTools>().paste = delayed_paste;
            let mut field = EditableText::new("original");
            field.queue_edit(TextEdit::Paste);
            let id = app.world_mut().spawn(field).id();
            let other = app.world_mut().spawn(EditableText::new("other-field")).id();
            app.world_mut()
                .resource_mut::<InputFocus>()
                .set(id, bevy::input_focus::FocusCause::Navigated);
            for _ in 0..200 {
                app.update();
                if PASTE_STARTED.load(Ordering::SeqCst) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            assert!(PASTE_STARTED.load(Ordering::SeqCst));
            match change {
                "text" => app
                    .world_mut()
                    .get_mut::<EditableText>(id)
                    .unwrap()
                    .editor
                    .set_text("new-value"),
                "caret" => app
                    .world_mut()
                    .get_mut::<EditableText>(id)
                    .unwrap()
                    .queue_edit(TextEdit::TextStart(false)),
                "ime" => app
                    .world_mut()
                    .get_mut::<EditableText>(id)
                    .unwrap()
                    .queue_edit(TextEdit::ImeSetCompose {
                        value: "候".into(),
                        cursor: None,
                    }),
                "focus" => {
                    let mut focus = app.world_mut().resource_mut::<InputFocus>();
                    focus.set(other, bevy::input_focus::FocusCause::Navigated);
                    focus.set(id, bevy::input_focus::FocusCause::Navigated);
                }
                "despawn" => {
                    app.world_mut().despawn(id);
                }
                _ => unreachable!(),
            }
            app.update();
            PASTE_RELEASE.store(true, Ordering::SeqCst);
            settle(&mut app);
            if let Some(field) = app.world().get::<EditableText>(id) {
                assert!(
                    !field.value().to_string().contains("stale-paste"),
                    "{change}"
                );
                assert_eq!(
                    field.value().to_string(),
                    if change == "text" {
                        "new-value"
                    } else {
                        "original"
                    }
                );
            }
            assert_eq!(
                app.world()
                    .get::<EditableText>(other)
                    .unwrap()
                    .value()
                    .to_string(),
                "other-field"
            );
        }
    }
}
