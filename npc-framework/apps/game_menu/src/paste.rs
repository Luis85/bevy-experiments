//! OS copy, cut and paste for the menu's text fields (Ruling R10).
//!
//! The menu's Bevy has no `system_clipboard` feature (its `clipboard-win`
//! dependency is BSL-1.0, which `deny.toml` does not allow), so `TextInput`'s
//! Ctrl+C / Ctrl+X / Ctrl+V (Cmd on macOS, and the Copy / Cut / Paste keys)
//! queue [`TextEdit::Copy`] / [`TextEdit::Cut`] / [`TextEdit::Paste`], which
//! would use Bevy's in-app buffer. [`FieldClipboardPlugin`] takes those edits
//! out of the field's queue before Bevy applies them (`PostUpdate`) and uses
//! the OS clipboard instead, through the shared
//! `testbed_content::explorer::clipboard` (the platform's tools, at most 5 s
//! each) on a background thread, never on the frame:
//!
//! - **Copy / Cut** write the selection with `clipboard::copy`. Cut's edit
//!   becomes a [`TextEdit::Delete`] of the selection, so the text leaves the
//!   field at once, as with Bevy's own Cut; a copy that fails is logged. The
//!   selection is the one of the edits applied so far (a Select All queued in
//!   the same frame as the Copy is not seen yet).
//! - **Paste** reads `clipboard::paste` and inserts the text at the caret
//!   (replacing a selection) when it arrives: the OS text replaces the in-app
//!   paste, never both. Single-line fields get the first non-blank line only.

use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use bevy::text::{EditableText, TextEdit};
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
        result: Slot<Result<String, String>>,
    },
    /// A write of a copied or cut selection.
    Copy(Slot<Result<(), String>>),
}

/// The clipboard operations in flight (the power settings keep the menu
/// awake while there are any).
#[derive(Resource, Default)]
pub struct ClipboardJobs(Vec<Job>);

impl ClipboardJobs {
    /// Nothing in flight.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// OS copy, cut and paste for every [`EditableText`] (see the module docs).
pub struct FieldClipboardPlugin;

impl Plugin for FieldClipboardPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ClipboardTools>()
            .init_resource::<ClipboardJobs>()
            .add_systems(Update, (take_clipboard_edits, finish_jobs).chain());
    }
}

/// Takes Copy, Cut and Paste out of each field's queue (queued in
/// `PreUpdate` by `TextInput`, applied in `PostUpdate`) and starts the OS
/// write or read for them.
fn take_clipboard_edits(
    mut fields: Query<(Entity, &mut EditableText)>,
    tools: Res<ClipboardTools>,
    mut jobs: ResMut<ClipboardJobs>,
) {
    for (field, mut text) in &mut fields {
        if !text
            .pending_edits
            .iter()
            .any(|e| matches!(e, TextEdit::Copy | TextEdit::Cut | TextEdit::Paste))
        {
            continue;
        }
        let text = text.as_mut();
        let selected = text.editor.selected_text().map(str::to_owned);
        if let Some(copied) = take_copy_edits(&mut text.pending_edits, selected.as_deref()) {
            let copy = tools.copy;
            jobs.0.push(Job::Copy(on_thread(move || copy(&copied))));
        }
        if text
            .pending_edits
            .iter()
            .any(|e| matches!(e, TextEdit::Paste))
        {
            text.pending_edits.retain(|e| !matches!(e, TextEdit::Paste));
            let paste = tools.paste;
            jobs.0.push(Job::Paste {
                field,
                result: on_thread(paste),
            });
        }
    }
}

/// Inserts each finished read at its field's caret (a field that is gone,
/// e.g. after a screen change, drops it) and logs a failed read or write.
fn finish_jobs(mut jobs: ResMut<ClipboardJobs>, mut fields: Query<&mut EditableText>) {
    jobs.0.retain(|job| match job {
        Job::Paste { field, result } => {
            let Some(result) = take(result) else {
                return true;
            };
            match result {
                Ok(raw) => {
                    if let Ok(mut text) = fields.get_mut(*field) {
                        let insert = paste_text(&raw, !text.allow_newlines);
                        if !insert.is_empty() {
                            text.queue_edit(TextEdit::Insert(insert.into()));
                        }
                    }
                }
                Err(e) => warn!("game_menu: cannot read the clipboard: {e}"),
            }
            false
        }
        Job::Copy(result) => {
            let Some(result) = take(result) else {
                return true;
            };
            if let Err(e) = result {
                warn!("game_menu: cannot write the clipboard: {e}");
            }
            false
        }
    });
}

#[cfg(test)]
mod tests {
    use bevy::text::{DEFAULT_FONT_DATA, Font, FontCx, GenericFontFamily, LayoutCx};

    use super::*;

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

    fn fake_copy(text: &str) -> Result<(), String> {
        COPIED.lock().unwrap().push(text.to_owned());
        Ok(())
    }

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(ClipboardTools {
                paste: fake_paste,
                copy: fake_copy,
            })
            .add_plugins(FieldClipboardPlugin);
        app
    }

    /// Updates until the clipboard jobs are done.
    fn settle(app: &mut App) {
        for _ in 0..400 {
            app.update();
            if app.world().resource::<ClipboardJobs>().is_empty() {
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
        field.pending_edits.clear();
        // What `TextInput` queues for Ctrl+V.
        field.queue_edit(TextEdit::Paste);
        let id = app.world_mut().spawn(field).id();
        settle(&mut app);
        let edits = &app.world().get::<EditableText>(id).unwrap().pending_edits;
        assert_eq!(edits, &vec![TextEdit::Insert("Hollowmere".into())]);
    }

    #[test]
    fn ctrl_c_and_ctrl_x_write_the_selection_to_the_os_clipboard() {
        let mut app = app();
        let mut field = EditableText::new("Wren Ashdown");
        field.pending_edits.clear();
        // A selection, as the field's earlier edits leave it (laid out with
        // Bevy's embedded font standing in for every generic family).
        let (mut fonts, mut layout) = (FontCx::default(), LayoutCx::default());
        let font = Font::from_bytes(DEFAULT_FONT_DATA.to_vec());
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
        field
            .editor
            .driver(&mut fonts, &mut layout.0)
            .select_byte_range(5, 12);
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
        assert_eq!(text.pending_edits, vec![TextEdit::Delete]);
    }
}
