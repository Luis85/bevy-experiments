//! OS copy, cut and paste for the F7 notes editor's text fields, the way the
//! game menu does it (`apps/game_menu/src/paste.rs`, Ruling R10).
//!
//! The testbed's Bevy has no `system_clipboard` feature (its `clipboard-win`
//! dependency is BSL-1.0, which `deny.toml` does not allow), so `TextInput`'s
//! Ctrl+C / Ctrl+X / Ctrl+V (and the Copy / Cut / Paste keys) queue
//! [`TextEdit::Copy`] / [`TextEdit::Cut`] / [`TextEdit::Paste`] for Bevy's
//! in-app buffer. These systems take those edits out of a field's queue in
//! `Update` (before Bevy applies the queue in `PostUpdate`) and use the OS
//! clipboard instead, through `testbed_content::explorer::clipboard`, on a
//! background thread:
//!
//! - **Copy / Cut** write the selection; Cut's edit becomes a
//!   [`TextEdit::Delete`] of the selection, so the text leaves the field at
//!   once. A failed write is logged.
//! - **Paste** reads the clipboard and inserts the text at the caret when it
//!   arrives ([`clipboard::paste_text`]: one line for a single-line field).

use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use bevy::text::{EditableText, TextEdit};
use testbed_content::explorer::clipboard;

/// Takes [`TextEdit::Copy`] and [`TextEdit::Cut`] out of `edits` and returns
/// the text to put on the OS clipboard (`selected`) when one was queued. The
/// first Cut becomes a [`TextEdit::Delete`] when there is a selection.
pub(crate) fn take_copy_edits(edits: &mut Vec<TextEdit>, selected: Option<&str>) -> Option<String> {
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
pub(crate) struct ClipboardTools {
    pub paste: fn() -> Result<String, String>,
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

type Slot<T> = Arc<Mutex<Option<T>>>;

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

fn take<T>(slot: &Slot<T>) -> Option<T> {
    slot.lock().ok().and_then(|mut s| s.take())
}

enum Job {
    Paste {
        field: Entity,
        result: Slot<Result<String, String>>,
    },
    Copy(Slot<Result<(), String>>),
}

/// The clipboard operations in flight.
#[derive(Resource, Default)]
pub(crate) struct ClipboardJobs(Vec<Job>);

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<ClipboardTools>()
        .init_resource::<ClipboardJobs>()
        .add_systems(Update, (take_clipboard_edits, finish_jobs).chain());
}

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

fn finish_jobs(mut jobs: ResMut<ClipboardJobs>, mut fields: Query<&mut EditableText>) {
    jobs.0.retain(|job| match job {
        Job::Paste { field, result } => {
            let Some(result) = take(result) else {
                return true;
            };
            match result {
                Ok(raw) => {
                    if let Ok(mut text) = fields.get_mut(*field) {
                        let insert = clipboard::paste_text(&raw, !text.allow_newlines);
                        if !insert.is_empty() {
                            text.queue_edit(TextEdit::Insert(insert.into()));
                        }
                    }
                }
                Err(e) => warn!("testbed_view: cannot read the clipboard: {e}"),
            }
            false
        }
        Job::Copy(result) => {
            let Some(result) = take(result) else {
                return true;
            };
            if let Err(e) = result {
                warn!("testbed_view: cannot write the clipboard: {e}");
            }
            false
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_paste() -> Result<String, String> {
        Ok("first line\r\nsecond line\r\n".to_owned())
    }

    fn fake_copy(_: &str) -> Result<(), String> {
        Ok(())
    }

    #[test]
    fn os_paste_replaces_the_in_app_paste() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(ClipboardTools {
                paste: fake_paste,
                copy: fake_copy,
            })
            .add_plugins(plugin);
        let mut single = EditableText::new("");
        single.pending_edits.clear();
        single.queue_edit(TextEdit::Paste);
        let mut multi = EditableText {
            allow_newlines: true,
            ..EditableText::new("")
        };
        multi.pending_edits.clear();
        multi.queue_edit(TextEdit::Paste);
        let (single, multi) = (
            app.world_mut().spawn(single).id(),
            app.world_mut().spawn(multi).id(),
        );
        for _ in 0..400 {
            app.update();
            if app.world().resource::<ClipboardJobs>().0.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let edits = |e| {
            app.world()
                .get::<EditableText>(e)
                .unwrap()
                .pending_edits
                .clone()
        };
        assert_eq!(edits(single), vec![TextEdit::Insert("first line".into())]);
        assert_eq!(
            edits(multi),
            vec![TextEdit::Insert("first line\nsecond line".into())]
        );
    }

    #[test]
    fn copy_and_cut_take_the_selection() {
        let mut edits = vec![TextEdit::Left(false), TextEdit::Cut, TextEdit::Copy];
        assert_eq!(take_copy_edits(&mut edits, Some("ash")), Some("ash".into()));
        assert_eq!(edits, vec![TextEdit::Left(false), TextEdit::Delete]);
        let mut edits = vec![TextEdit::Cut];
        assert_eq!(take_copy_edits(&mut edits, None), None);
        assert!(edits.is_empty());
    }
}
