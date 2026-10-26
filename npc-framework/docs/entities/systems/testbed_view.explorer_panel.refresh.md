---
type: System
---
# `refresh` (F7 notes editor)

**Crate:** `testbed_view` · **File:** [`content/testbed_view/src/explorer_panel.rs`](../../../content/testbed_view/src/explorer_panel.rs) · **Plugin:** `TestbedViewPlugin` (`explorer_panel::plugin`)

Shows the F7 notes editor in explorer mode: the agent's Diary, Memory and Proposals as tabs, each a
scrollable list newest (highest id) first with title (`[status]` for a proposal), `#id`, created and updated
stamps (`t<tick> <date>`), `prompted by`, the body, and per proposal a button per status (`new`, `liked`,
`pursuing`, `rejected`). `refresh` sets the `Display` of the panel (from `TestbedPanels.explorer`, so
`--panels explorer` works while input is gated by the prompt modal), of the form box and of its "Now in the
file:" block; while open it re-lists the shown book in the frame the panel opens and then on a 1 s timer,
writes the message line and the stale-save block, and rebuilds the list when `NotesEditor.rows_dirty`. The
list comes from the shared `NotesStores` (`store_for(ExplorerFiles.dir)`, `list(book)`), which re-reads only
files whose modification time or size changed, so agent writes and hand edits appear within about a second.
(`explorer_modal.rs` has its own `refresh`; this is the panel's.)

Its siblings do the editing. `on_activate` (observer) turns a button's `Activate` into a request;
`apply_requests` handles them: switch book, select an entry (click its title), Add / Edit open a title field
and an 8-line body field (Bevy `TextInput`), Save calls the store's `add` or `update` as `Writer::Player`,
stamped with `SimClock.tick` and the host's `NotesClock` date, Cancel drops the form, a status button sets a
proposal's status. An update passes the `updated` stamp read when editing began; if the entry changed
meanwhile the save is refused (`stale_save_note`), the form keeps the player's text and shows the file's
current version, and a second Save replaces it (a refused status change shows `STALE_NOTE`). `paint` colours
the buttons (pressed, hovered, active tab / selected entry / current status).

## Schedule

- **Schedule:** `Update`; `spawn` (the panel node) in `Startup`
- **Set:** `(apply_requests, refresh, paint).chain()` in `ViewSystems::Present`. `keys` (F7 toggles
  `TestbedPanels.explorer`; Tab, while open, shows the next book) is in `ViewSystems::Input`,
  `.before(apply_requests)`. `field_tab` (Tab moves between the two fields; only while `editor_typing`) and
  `shield_world` (while the pointer is over the open panel, clears `ButtonInput<MouseButton>` and the wheel
  delta so clicks and scrolling do not reach the world) run `.before(ViewSystems::Input)`, outside it.
- **Ordering:** everything (`spawn` included) runs only with `resource_exists::<ExplorerFiles>` (the app
  inserts it in explorer mode, `--explorer-home`). The plugin configures `ViewSystems::Input` with
  `run_if(not(editor_typing))` (a `NoteField` has the `InputFocus`), so while the player types no game key
  runs (camera, orders, panel toggles, the app's speed keys), F7 and Tab included; with the prompt modal,
  `ViewSystems::Input` is also gated by `not(modal_open)`. The plugin adds `field_clipboard::plugin`
  (`(take_clipboard_edits, finish_jobs).chain()` in `Update`, no set: OS copy, cut and paste for the fields,
  on a background thread, through `testbed_content::explorer::clipboard`).

## Reads / writes

| Access | Data |
|---|---|
| reads | `Time`, `TestbedPanels.explorer`, `ExplorerFiles`; `SimClock`, `NotesClock` (optional, in `apply_requests`); the fields' `EditableText`; the agent folder's `diary.md`, `memory.md`, `proposals/` (through `NotesStores`) |
| writes | `PanelState` (timer, `was_open`), `NotesEditor` (book, notes, selection, form, message), `NotesStores`; the panel's `Node.display`, `Text`, list children and the form's fields (view only); the notes files (`add` / `update` as `Writer::Player`, in `apply_requests`; the store may also write back ids for an old folder or a revision for a hand edit) |
| messages / events | observes `Activate` (`on_activate`) |

## Determinism

View-only towards the sim: reads the tick for stamps, never writes sim state. Notes are host-side files, not
hashed and not in the world snapshot; the panel writes nothing else. Wall-clock dates and file times are fine
here, outside the sim.

## Performance

- **Mechanic tag:** `view.explorer_panel` (via `testbed_view::perf_tags` prefix `testbed_view::explorer_panel`;
  the clipboard systems `view.field_clipboard`) · **Budget:** none · no span. The store touches disk at most
  once per second while the panel is open, plus once per write.

## Related

- [`testbed_view.update_text`](testbed_view.update_text.md) — F1.
- `crates/npc_agent_io/src/notes.rs` — the notes store, `NotesDirs`, `NotesClock`, `NotesStores` (shared
  with the `agent/notes/*` and `notes/*` methods).
- `docs/agent-experiments.md` — explorer mode and agent folders.
