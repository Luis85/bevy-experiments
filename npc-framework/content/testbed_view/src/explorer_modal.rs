//! The explorer's prompt modal (game menu spec G4): a centred panel with a
//! title, the agent prompt (scrollable, monospace 13 px, at most 900 px
//! wide), notes, **Copy** (the prompt to the clipboard; the label reads
//! "Copied" for [`COPIED_FOR`]) and **OK** (closes it and sends
//! [`PromptModalClosed`]).
//!
//! The app inserts a [`PromptModal`] and owns what closing means (the
//! explorer unpauses the sim on [`PromptModalClosed`]); this module never
//! touches sim state. While the modal is open the game's input
//! ([`ViewSystems::Input`]: the RTS command UI, order and camera keys, panel
//! toggles and the app's speed keys) does not run, so no click or key
//! reaches the world behind it. OK takes effect after that frame's input,
//! so the click or key that closes the modal never reaches the game either.
//!
//! Copy hands the text to `testbed_content::explorer::clipboard::copy` (the
//! platform's copy tool: `clip.exe`, `pbcopy`, or `wl-copy` / `xclip` /
//! `xsel`) on a background thread; "Copied" shows only when it reports
//! success. A copy that has not reported after [`COPY_TIMEOUT`], or whose
//! thread panicked, ends in the failure note naming the prompt file, and
//! Copy works again.
//!
//! When the prompt is taller than its scroll area, a hint under it says so
//! (Copy still takes all of it).
//!
//! The text uses Bevy's default font (Fira Mono, a monospace subset that has
//! printable ASCII only: titles and notes stay ASCII).

use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bevy::input_focus::AutoFocus;
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui::Pressed;
use bevy::ui_widgets::{
    Activate, Button as WidgetButton, ButtonPlugin, ScrollArea, ScrollAreaPlugin,
};
use npc_framework::view_helpers::ViewSystems;

/// How long the Copy button reads "Copied".
pub const COPIED_FOR: Duration = Duration::from_secs(2);
/// The panel's maximum width, px.
pub const MAX_WIDTH_PX: f32 = 900.0;
/// The prompt's font size, px.
pub const TEXT_PX: f32 = 13.0;
/// How long Copy waits for the clipboard before it shows the failure note.
pub const COPY_TIMEOUT: Duration = Duration::from_secs(7);

const COPY: &str = "Copy";
const COPIED: &str = "Copied";
/// Under the prompt when it does not fit (the default font is ASCII only).
const SCROLL_HINT: &str = "scroll for more - Copy takes all of it";

/// What the modal shows. Insert it with `open: true` to show it; OK sets
/// `open` to false (and sends [`PromptModalClosed`]).
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct PromptModal {
    /// The panel's title.
    pub title: String,
    /// The text Copy puts on the clipboard (shown scrollable).
    pub text: String,
    /// Lines under the text (the port, a missing tool, a load report, ..).
    pub notes: Vec<String>,
    /// Shown while true.
    pub open: bool,
    /// The file that holds `text`, named when the clipboard fails.
    pub file: Option<PathBuf>,
}

/// Sent once when OK closes the modal.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PromptModalClosed;

/// The modal's systems in `Update` (after [`ViewSystems::Input`]).
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ExplorerModalSystems;

/// The prompt modal (see the module docs). Adds `ui_widgets`' button and
/// scroll-area observers when they are missing.
pub struct ExplorerModalPlugin;

impl Plugin for ExplorerModalPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<ButtonPlugin>() {
            app.add_plugins(ButtonPlugin);
        }
        if !app.is_plugin_added::<ScrollAreaPlugin>() {
            app.add_plugins(ScrollAreaPlugin);
        }
        app.init_resource::<PromptModal>()
            .init_resource::<ModalUi>()
            .add_message::<PromptModalClosed>()
            .configure_sets(Update, ViewSystems::Input.run_if(not(modal_open)))
            .configure_sets(Update, ExplorerModalSystems.after(ViewSystems::Input))
            .add_systems(
                Update,
                (apply_close, sync_modal, poll_copy, refresh, scroll_hint)
                    .chain()
                    .in_set(ExplorerModalSystems),
            );
    }
}

/// Run condition: a [`PromptModal`] is open.
pub fn modal_open(modal: Option<Res<PromptModal>>) -> bool {
    modal.is_some_and(|m| m.open)
}

/// How Copy reaches the clipboard (a test swaps in a fake). It runs on a
/// background thread.
#[derive(Resource, Clone, Copy)]
struct PromptClipboard(fn(&str) -> Result<(), String>);

/// How long a copy may take before Copy gives up on it (a test shortens it).
#[derive(Resource, Clone, Copy)]
struct CopyTimeout(Duration);

/// A copy running on a background thread; `Some` once it finished.
type CopyJob = Arc<Mutex<Option<Result<(), String>>>>;

/// The modal's own state.
#[derive(Resource, Default, Debug)]
struct ModalUi {
    /// The spawned panel.
    root: Option<Entity>,
    /// When Copy last succeeded (`Time<Real>` elapsed).
    copied_at: Option<Duration>,
    /// The note of the last failed copy.
    copy_error: Option<String>,
    /// The copy in flight and when it started (`Time<Real>` elapsed).
    pending: Option<(CopyJob, Duration)>,
    /// OK was activated; applied after this frame's input.
    close_requested: bool,
}

#[derive(Component)]
struct ModalRoot;
#[derive(Component)]
struct CopyButton;
#[derive(Component)]
struct OkButton;
#[derive(Component)]
struct CopyLabel;
#[derive(Component)]
struct NotesText;
/// The prompt's scroll area.
#[derive(Component)]
struct PromptScroll;
/// [`SCROLL_HINT`], shown while the prompt does not fit.
#[derive(Component)]
struct ScrollHint;

const BUTTON: Color = Color::srgb(0.20, 0.24, 0.30);
const BUTTON_HOVER: Color = Color::srgb(0.27, 0.32, 0.40);
const BUTTON_PRESSED: Color = Color::srgb(0.14, 0.17, 0.22);

/// The Copy label: "Copied" for [`COPIED_FOR`] after a copy at `copied_at`.
fn copy_label(copied_at: Option<Duration>, now: Duration) -> &'static str {
    match copied_at {
        Some(at) if now.saturating_sub(at) < COPIED_FOR => COPIED,
        _ => COPY,
    }
}

/// The note for a failed copy.
fn copy_failed_note(file: Option<&PathBuf>) -> String {
    let file = file.map_or_else(|| "the prompt file".to_owned(), |f| f.display().to_string());
    format!("copy failed: select the text in {file} instead")
}

/// The notes text: the modal's notes plus a failed copy's note.
fn notes_text(modal: &PromptModal, copy_error: Option<&str>) -> String {
    modal
        .notes
        .iter()
        .map(String::as_str)
        .chain(copy_error)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Starts the copy on a background thread (one at a time).
fn on_copy(
    _: On<Activate>,
    modal: Res<PromptModal>,
    clipboard: Option<Res<PromptClipboard>>,
    time: Option<Res<Time<Real>>>,
    mut ui: ResMut<ModalUi>,
) {
    if ui.pending.is_some() {
        return;
    }
    let now = time.map(|t| t.elapsed()).unwrap_or_default();
    let write = clipboard.map_or(
        testbed_content::explorer::clipboard::copy as fn(&str) -> _,
        |c| c.0,
    );
    let job = CopyJob::default();
    let (slot, text) = (Arc::clone(&job), modal.text.clone());
    let spawned = std::thread::Builder::new()
        .name("prompt-copy".into())
        .spawn(move || {
            // A panicking copy reports an error instead of never reporting.
            let result = std::panic::catch_unwind(AssertUnwindSafe(|| write(&text)))
                .unwrap_or_else(|_| Err("the copy thread panicked".to_owned()));
            if let Ok(mut done) = slot.lock() {
                *done = Some(result);
            }
        });
    match spawned {
        Ok(_) => ui.pending = Some((job, now)),
        Err(e) => finish_copy(&mut ui, &modal, Err(e.to_string()), now),
    }
}

/// Records a finished copy: "Copied" from `now`, or the failure note.
fn finish_copy(ui: &mut ModalUi, modal: &PromptModal, result: Result<(), String>, now: Duration) {
    match result {
        Ok(()) => {
            ui.copied_at = Some(now);
            ui.copy_error = None;
        }
        Err(e) => {
            warn!("testbed_view: prompt copy failed: {e}");
            ui.copied_at = None;
            ui.copy_error = Some(copy_failed_note(modal.file.as_ref()));
        }
    }
}

/// Picks up a finished background copy, or gives up on one that has not
/// reported within the [`CopyTimeout`] (its late result is dropped).
fn poll_copy(
    modal: Res<PromptModal>,
    mut ui: ResMut<ModalUi>,
    time: Option<Res<Time<Real>>>,
    timeout: Option<Res<CopyTimeout>>,
) {
    let Some((job, started)) = ui.pending.as_ref() else {
        return;
    };
    let now = time.map(|t| t.elapsed()).unwrap_or_default();
    let limit = timeout.map_or(COPY_TIMEOUT, |t| t.0);
    let done = match job.lock() {
        Ok(mut slot) => slot.take(),
        Err(_) => Some(Err("the copy thread panicked".to_owned())),
    };
    let result = match done {
        Some(result) => result,
        None if now.saturating_sub(*started) >= limit => Err(format!(
            "no answer from the clipboard after {} s",
            limit.as_secs_f32()
        )),
        None => return,
    };
    ui.pending = None;
    finish_copy(&mut ui, &modal, result, now);
}

fn on_ok(_: On<Activate>, mut ui: ResMut<ModalUi>) {
    ui.close_requested = true;
}

/// OK takes effect here, after the frame's input (which was skipped).
fn apply_close(
    mut ui: ResMut<ModalUi>,
    mut modal: ResMut<PromptModal>,
    mut closed: MessageWriter<PromptModalClosed>,
) {
    if !std::mem::take(&mut ui.close_requested) {
        return;
    }
    if modal.open {
        modal.open = false;
        closed.write(PromptModalClosed);
    }
}

/// Spawns the panel when the modal opens (or changes), despawns it when it
/// closes.
fn sync_modal(mut commands: Commands, modal: Res<PromptModal>, mut ui: ResMut<ModalUi>) {
    let rebuild = modal.is_changed() && ui.root.is_some();
    if (!modal.open || rebuild)
        && let Some(root) = ui.root.take()
    {
        commands.entity(root).despawn();
        ui.copied_at = None;
        ui.copy_error = None;
        ui.pending = None;
    }
    if modal.open && ui.root.is_none() {
        ui.root = Some(spawn_panel(&mut commands, &modal));
    }
}

fn spawn_panel(commands: &mut Commands, modal: &PromptModal) -> Entity {
    let font = |px: f32| TextFont {
        font_size: FontSize::Px(px),
        ..default()
    };
    let notes = notes_text(modal, None);
    commands
        .spawn((
            Name::new("explorer prompt modal"),
            ModalRoot,
            // Full screen: dims the world and takes the pointer.
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                top: px(0),
                width: percent(100),
                height: percent(100),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.45)),
            GlobalZIndex(1000),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    width: percent(90),
                    max_width: px(MAX_WIDTH_PX),
                    max_height: percent(88),
                    padding: UiRect::all(px(16)),
                    row_gap: px(10),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(6)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.08, 0.09, 0.11)),
                BorderColor::all(Color::srgb(0.35, 0.40, 0.48)),
            ))
            .with_children(|panel| {
                panel.spawn((
                    Text::new(modal.title.clone()),
                    font(18.0),
                    TextColor(Color::srgb(0.97, 0.95, 0.85)),
                ));
                panel
                    .spawn((
                        Node {
                            overflow: Overflow::scroll_y(),
                            flex_grow: 1.0,
                            flex_shrink: 1.0,
                            min_height: px(0),
                            padding: UiRect::all(px(8)),
                            border_radius: BorderRadius::all(px(4)),
                            ..default()
                        },
                        PromptScroll,
                        ScrollArea,
                        BackgroundColor(Color::srgb(0.04, 0.05, 0.06)),
                    ))
                    .with_child((
                        Text::new(modal.text.clone()),
                        font(TEXT_PX),
                        TextColor(Color::srgb(0.88, 0.90, 0.88)),
                    ));
                panel.spawn((
                    ScrollHint,
                    Text::new(SCROLL_HINT),
                    font(12.0),
                    TextColor(Color::srgb(0.62, 0.68, 0.74)),
                    Node {
                        display: Display::None,
                        ..default()
                    },
                ));
                panel.spawn((
                    NotesText,
                    Text::new(notes),
                    font(12.0),
                    TextColor(Color::srgb(0.95, 0.80, 0.45)),
                ));
                panel
                    .spawn(Node {
                        justify_content: JustifyContent::FlexEnd,
                        column_gap: px(10),
                        ..default()
                    })
                    .with_children(|row| {
                        row.spawn((
                            Name::new("prompt modal Copy"),
                            CopyButton,
                            button_node(),
                            BackgroundColor(BUTTON),
                        ))
                        .insert((WidgetButton, Hovered::default()))
                        .with_child((CopyLabel, Text::new(COPY), font(14.0)))
                        .observe(on_copy);
                        row.spawn((
                            Name::new("prompt modal OK"),
                            OkButton,
                            button_node(),
                            BackgroundColor(BUTTON),
                        ))
                        .insert((WidgetButton, Hovered::default(), AutoFocus))
                        .with_child((Text::new("OK"), font(14.0)))
                        .observe(on_ok);
                    });
            });
        })
        .id()
}

fn button_node() -> Node {
    Node {
        padding: UiRect::axes(px(18), px(6)),
        min_width: px(84),
        justify_content: JustifyContent::Center,
        border: UiRect::all(px(1)),
        border_radius: BorderRadius::all(px(4)),
        ..default()
    }
}

/// The Copy label, the notes and the button colours.
fn refresh(
    modal: Res<PromptModal>,
    ui: Res<ModalUi>,
    time: Option<Res<Time<Real>>>,
    mut labels: Query<&mut Text, (With<CopyLabel>, Without<NotesText>)>,
    mut notes: Query<&mut Text, (With<NotesText>, Without<CopyLabel>)>,
    mut buttons: Query<
        (&mut BackgroundColor, Option<&Hovered>, Has<Pressed>),
        Or<(With<CopyButton>, With<OkButton>)>,
    >,
) {
    if ui.root.is_none() {
        return;
    }
    let now = time.map(|t| t.elapsed()).unwrap_or_default();
    let label = copy_label(ui.copied_at, now);
    for mut text in &mut labels {
        if text.0 != label {
            label.clone_into(&mut text.0);
        }
    }
    let want = notes_text(&modal, ui.copy_error.as_deref());
    for mut text in &mut notes {
        if text.0 != want {
            text.0.clone_from(&want);
        }
    }
    for (mut bg, hovered, pressed) in &mut buttons {
        let color = if pressed {
            BUTTON_PRESSED
        } else if hovered.is_some_and(|h| h.0) {
            BUTTON_HOVER
        } else {
            BUTTON
        };
        if bg.0 != color {
            bg.0 = color;
        }
    }
}

/// Whether a scroll area can scroll vertically: its content is taller than
/// the area (by more than a physical pixel of rounding), which is the range
/// the `ScrollArea` widget scrolls through.
fn can_scroll_y(area: &ComputedNode) -> bool {
    area.content_size.y - area.size.y > 1.0
}

/// Shows [`SCROLL_HINT`] while the prompt does not fit its scroll area
/// (layout sizes, so it follows window resizes).
fn scroll_hint(
    areas: Query<&ComputedNode, With<PromptScroll>>,
    mut hints: Query<&mut Node, With<ScrollHint>>,
) {
    let want = if areas.iter().any(can_scroll_y) {
        Display::Flex
    } else {
        Display::None
    };
    for mut node in &mut hints {
        if node.display != want {
            node.display = want;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What the fake clipboard received.
    static PASTED: Mutex<Option<String>> = Mutex::new(None);

    fn fake_clipboard(text: &str) -> Result<(), String> {
        *PASTED.lock().unwrap() = Some(text.to_owned());
        Ok(())
    }

    fn broken_clipboard(_: &str) -> Result<(), String> {
        Err("xclip exited with exit status: 1".into())
    }

    /// Updates until the background copy has been picked up (a few seconds
    /// at most: the copy and its timeout run in real time).
    fn settle(app: &mut App) {
        for _ in 0..1500 {
            app.update();
            if app.world().resource::<ModalUi>().pending.is_none() {
                return;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        panic!("the copy never finished");
    }

    /// A copy tool that hangs well past the (shortened) timeout.
    fn hung_clipboard(_: &str) -> Result<(), String> {
        std::thread::sleep(Duration::from_secs(3));
        Ok(())
    }

    fn panicking_clipboard(_: &str) -> Result<(), String> {
        panic!("the copy tool blew up");
    }

    /// Run by hand: `cargo test -p testbed_view os_clipboard -- --ignored`.
    /// Copy goes through the real OS clipboard (the shared
    /// `testbed_content::explorer::clipboard::copy`); read back with its `paste`.
    #[test]
    #[ignore = "overwrites the OS clipboard"]
    fn os_clipboard_round_trip() {
        let mut app = app(true);
        app.world_mut().resource_mut::<PromptModal>().text =
            "npc explorer \u{e9}\u{20ac} line 1\nline 2".into();
        app.update();
        activate::<CopyButton>(&mut app);
        settle(&mut app);
        app.update();
        assert_eq!(text_of::<CopyLabel>(&mut app), COPIED);
        let back = testbed_content::explorer::clipboard::paste().unwrap();
        let text = app.world().resource::<PromptModal>().text.clone();
        assert_eq!(back.trim_end().replace("\r\n", "\n"), text);
    }

    fn app(open: bool) -> App {
        let mut app = App::new();
        // A fixed step per frame: the "Copied" window and the copy timeout
        // must not depend on how loaded the machine is between updates.
        app.add_plugins(MinimalPlugins)
            .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
                Duration::from_millis(16),
            ))
            .add_plugins(ExplorerModalPlugin)
            .insert_resource(PromptModal {
                title: "Your agent's start prompt".into(),
                text: "You are Wren.\nConnect to http://127.0.0.1:15703.".into(),
                notes: vec!["remote: http://127.0.0.1:15703".into()],
                open,
                file: Some(PathBuf::from("C:/agents/wren/prompts/start.md")),
            });
        app.update();
        app
    }

    fn one<C: Component>(app: &mut App) -> Entity {
        let mut q = app.world_mut().query_filtered::<Entity, With<C>>();
        q.single(app.world()).expect("exactly one")
    }

    fn count<C: Component>(app: &mut App) -> usize {
        let mut q = app.world_mut().query_filtered::<(), With<C>>();
        q.iter(app.world()).count()
    }

    fn text_of<C: Component>(app: &mut App) -> String {
        let mut q = app.world_mut().query_filtered::<&Text, With<C>>();
        q.single(app.world()).expect("one text").0.clone()
    }

    /// `PromptModalClosed` messages read so far.
    #[derive(Resource, Default)]
    struct Closed(usize);

    fn count_closed(mut r: MessageReader<PromptModalClosed>, mut n: ResMut<Closed>) {
        n.0 += r.read().count();
    }

    fn closed_count(app: &App) -> usize {
        app.world().resource::<Closed>().0
    }

    fn activate<C: Component>(app: &mut App) {
        let entity = one::<C>(app);
        app.world_mut().trigger(Activate { entity });
        app.world_mut().flush();
    }

    #[test]
    fn open_modal_spawns_panel_and_blocks_input() {
        let mut app = app(true);
        assert_eq!(count::<ModalRoot>(&mut app), 1);
        assert_eq!(count::<CopyButton>(&mut app), 1);
        assert_eq!(count::<OkButton>(&mut app), 1);
        assert_eq!(text_of::<CopyLabel>(&mut app), COPY);
        assert_eq!(
            text_of::<NotesText>(&mut app),
            "remote: http://127.0.0.1:15703"
        );
        assert!(app.world_mut().run_system_cached(modal_open).unwrap());
    }

    #[test]
    fn closed_modal_spawns_nothing() {
        let mut app = app(false);
        assert_eq!(count::<ModalRoot>(&mut app), 0);
        assert!(!app.world_mut().run_system_cached(modal_open).unwrap());
    }

    #[test]
    fn copy_puts_text_on_clipboard_and_says_copied() {
        let mut app = app(true);
        app.insert_resource(PromptClipboard(fake_clipboard));
        activate::<CopyButton>(&mut app);
        settle(&mut app);
        app.update();
        let text = app.world().resource::<PromptModal>().text.clone();
        assert_eq!(PASTED.lock().unwrap().as_deref(), Some(&*text));
        assert_eq!(text_of::<CopyLabel>(&mut app), COPIED);
        // Copy does not close.
        assert!(app.world().resource::<PromptModal>().open);
        assert_eq!(count::<ModalRoot>(&mut app), 1);
    }

    #[test]
    fn copy_label_reverts_after_two_seconds() {
        let at = Duration::from_secs(10);
        assert_eq!(copy_label(None, at), COPY);
        assert_eq!(copy_label(Some(at), at), COPIED);
        assert_eq!(
            copy_label(Some(at), at + Duration::from_millis(1999)),
            COPIED
        );
        assert_eq!(copy_label(Some(at), at + COPIED_FOR), COPY);
    }

    #[test]
    fn copy_failure_names_the_file() {
        let mut app = app(true);
        app.insert_resource(PromptClipboard(broken_clipboard));
        activate::<CopyButton>(&mut app);
        settle(&mut app);
        app.update();
        assert_eq!(text_of::<CopyLabel>(&mut app), COPY);
        let notes = text_of::<NotesText>(&mut app);
        assert!(
            notes.ends_with(
                "copy failed: select the text in C:/agents/wren/prompts/start.md instead"
            ),
            "{notes}"
        );
        assert!(notes.starts_with("remote:"), "{notes}");
    }

    #[test]
    fn ok_closes_once_and_despawns() {
        let mut app = app(true);
        app.init_resource::<Closed>()
            .add_systems(Update, count_closed.after(ExplorerModalSystems));
        activate::<OkButton>(&mut app);
        // The request waits for the frame's input to pass.
        assert!(app.world().resource::<PromptModal>().open);
        app.update();
        assert!(!app.world().resource::<PromptModal>().open);
        assert_eq!(closed_count(&app), 1);
        assert_eq!(count::<ModalRoot>(&mut app), 0);
        assert_eq!(count::<CopyButton>(&mut app), 0);
        assert!(!app.world_mut().run_system_cached(modal_open).unwrap());
        app.update();
        app.update();
        assert_eq!(closed_count(&app), 1, "sent once");
    }

    /// A stand-in for the game's input systems.
    #[derive(Resource, Default)]
    struct InputRuns(u32);

    fn count_input(mut runs: ResMut<InputRuns>) {
        runs.0 += 1;
    }

    #[test]
    fn game_input_waits_while_open_and_on_the_closing_frame() {
        let mut app = app(true);
        app.init_resource::<InputRuns>()
            .add_systems(Update, count_input.in_set(ViewSystems::Input));
        app.update();
        app.update();
        assert_eq!(app.world().resource::<InputRuns>().0, 0);
        activate::<OkButton>(&mut app);
        app.update(); // the closing frame: input still skipped
        assert_eq!(app.world().resource::<InputRuns>().0, 0);
        app.update();
        assert_eq!(app.world().resource::<InputRuns>().0, 1);
    }

    #[test]
    fn reopening_respawns_the_panel() {
        let mut app = app(true);
        activate::<OkButton>(&mut app);
        app.update();
        assert_eq!(count::<ModalRoot>(&mut app), 0);
        app.world_mut().resource_mut::<PromptModal>().open = true;
        app.update();
        assert_eq!(count::<ModalRoot>(&mut app), 1);
    }

    const FAILED: &str = "copy failed: select the text in C:/agents/wren/prompts/start.md instead";

    #[test]
    fn a_copy_that_never_reports_times_out_and_copy_works_again() {
        let mut app = app(true);
        app.insert_resource(PromptClipboard(hung_clipboard))
            .insert_resource(CopyTimeout(Duration::from_millis(100)));
        activate::<CopyButton>(&mut app);
        assert!(app.world().resource::<ModalUi>().pending.is_some());
        // A second click while it hangs does not start another copy.
        activate::<CopyButton>(&mut app);
        settle(&mut app);
        app.update();
        assert_eq!(text_of::<CopyLabel>(&mut app), COPY);
        assert!(text_of::<NotesText>(&mut app).ends_with(FAILED));
        // Copy is not dead: the next copy runs and reports. The timeout goes
        // back to the real one first: it counts frames of the fixed 16 ms
        // step, while the copy thread runs in wall time, so the shortened
        // 100 ms (7 frames) could expire before a loaded machine had even
        // started the fake copy's thread.
        app.insert_resource(PromptClipboard(fake_clipboard))
            .insert_resource(CopyTimeout(COPY_TIMEOUT));
        activate::<CopyButton>(&mut app);
        settle(&mut app);
        app.update();
        assert_eq!(text_of::<CopyLabel>(&mut app), COPIED);
        assert!(!text_of::<NotesText>(&mut app).contains("copy failed"));
    }

    #[test]
    fn a_panicking_copy_ends_in_the_failure_note() {
        let mut app = app(true);
        app.insert_resource(PromptClipboard(panicking_clipboard));
        activate::<CopyButton>(&mut app);
        settle(&mut app);
        app.update();
        assert_eq!(text_of::<CopyLabel>(&mut app), COPY);
        assert!(text_of::<NotesText>(&mut app).ends_with(FAILED));
    }

    fn hint_display(app: &mut App) -> Display {
        let mut q = app.world_mut().query_filtered::<&Node, With<ScrollHint>>();
        q.single(app.world()).expect("one hint").display
    }

    /// Sets the scroll area's layout result (no layout runs headless).
    fn lay_out(app: &mut App, area_h: f32, content_h: f32) {
        let area = one::<PromptScroll>(app);
        let mut node = app.world_mut().get_mut::<ComputedNode>(area).unwrap();
        node.size = Vec2::new(800.0, area_h);
        node.content_size = Vec2::new(800.0, content_h);
        app.update();
    }

    #[test]
    fn the_scroll_hint_shows_only_while_the_prompt_overflows() {
        let mut app = app(true);
        assert_eq!(hint_display(&mut app), Display::None);
        lay_out(&mut app, 400.0, 1200.0);
        assert_eq!(hint_display(&mut app), Display::Flex);
        assert_eq!(text_of::<ScrollHint>(&mut app), SCROLL_HINT);
        lay_out(&mut app, 400.0, 400.5);
        assert_eq!(hint_display(&mut app), Display::None, "rounding fits");
        lay_out(&mut app, 400.0, 300.0);
        assert_eq!(hint_display(&mut app), Display::None);
    }
}
