//! Structured world presets editor. Files affect new worlds; saved worlds keep their own copy.
use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use bevy::prelude::*;
use bevy::text::{EditableText, TextEdit};
use bevy::ui::InteractionDisabled;
use bevy::ui_widgets::{Activate, ScrollArea};
use serde_json::Value;
use testbed_content::explorer::home::slug;
use testbed_content::worlds::{self, WorldPreset};

use crate::screens::{Action as MenuAction, Screen};
use crate::{AppSettings, ui};

const PAGES: [&str; 7] = [
    "Identity",
    "Hunger",
    "Thirst",
    "Energy",
    "Battery",
    "Environment",
    "Systems",
];
const NEEDS: [&str; 4] = ["hunger", "thirst", "energy", "battery"];

#[derive(Clone, Debug)]
struct Draft {
    values: BTreeMap<String, String>,
}
impl Draft {
    fn new(preset: &WorldPreset) -> Self {
        fn flatten(prefix: &str, value: &Value, out: &mut BTreeMap<String, String>) {
            if let Value::Object(fields) = value {
                for (key, value) in fields {
                    flatten(
                        &if prefix.is_empty() {
                            key.clone()
                        } else {
                            format!("{prefix}.{key}")
                        },
                        value,
                        out,
                    );
                }
            } else {
                out.insert(
                    prefix.into(),
                    value
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| value.to_string()),
                );
            }
        }
        let mut values = BTreeMap::new();
        flatten(
            "",
            &serde_json::to_value(preset).expect("WorldPreset serializes"),
            &mut values,
        );
        Self { values }
    }
    fn text(&self, key: &str) -> &str {
        self.values.get(key).map(String::as_str).unwrap_or_default()
    }
    fn parse(&self) -> Result<WorldPreset, Vec<String>> {
        let mut value =
            serde_json::to_value(worlds::plain_preset()).expect("WorldPreset serializes");
        let mut errors = Vec::new();
        for (key, text) in &self.values {
            let pointer = format!("/{}", key.replace('.', "/"));
            let Some(slot) = value.pointer_mut(&pointer) else {
                continue;
            };
            if slot.is_string() {
                *slot = Value::String(text.clone());
            } else {
                match serde_json::from_str::<Value>(text) {
                    Ok(next)
                        if (slot.is_boolean() && next.is_boolean())
                            || (slot.is_number() && next.as_i64().is_some()) =>
                    {
                        *slot = next
                    }
                    _ => errors.push(format!("{key}: enter a whole number")),
                }
            }
        }
        if !errors.is_empty() {
            return Err(errors);
        }
        let preset: WorldPreset = serde_json::from_value(value).map_err(|e| vec![e.to_string()])?;
        worlds::validate(&preset)?;
        Ok(preset)
    }
}

#[derive(Clone)]
struct PresetFile {
    path: PathBuf,
    preset: Result<WorldPreset, String>,
}
fn entries(dir: &Path) -> Vec<PresetFile> {
    worlds::load_worlds(dir)
        .into_iter()
        .map(|(path, preset)| PresetFile { path, preset })
        .collect()
}
#[derive(Resource)]
struct WorldEditor {
    dir: PathBuf,
    entries: Vec<PresetFile>,
    selected: Option<PathBuf>,
    original: Option<Vec<u8>>,
    loaded_name: String,
    draft: Draft,
    baseline: Draft,
    page: usize,
    rebuild: bool,
    message: String,
    overwrite: Option<(PathBuf, Vec<u8>)>,
    discard: Option<WorldAction>,
}
impl WorldEditor {
    fn new(dir: PathBuf) -> Self {
        let draft = Draft::new(&worlds::plain_preset());
        let mut editor = Self {
            entries: entries(&dir),
            dir,
            selected: None,
            original: None,
            loaded_name: String::new(),
            baseline: draft.clone(),
            draft,
            page: 0,
            rebuild: true,
            message: String::new(),
            overwrite: None,
            discard: None,
        };
        if let Some(index) = editor.entries.iter().position(|e| e.preset.is_ok()) {
            editor.select(index);
        }
        editor
    }
    fn select(&mut self, index: usize) {
        let Some(entry) = self.entries.get(index) else {
            return;
        };
        self.selected = Some(entry.path.clone());
        self.original = std::fs::read(&entry.path).ok();
        match &entry.preset {
            Ok(p) => {
                self.draft = Draft::new(p);
                self.loaded_name = p.name.clone();
                self.message = String::new();
            }
            Err(e) => {
                self.draft = Draft::new(&worlds::plain_preset());
                self.loaded_name = String::new();
                self.message = format!(
                    "Cannot edit {}: {e}. Fix this file externally, or choose New.",
                    entry.path.display()
                );
            }
        }
        self.baseline = self.draft.clone();
        self.overwrite = None;
        self.rebuild = true;
    }
    fn source_error(&self) -> Option<&str> {
        self.selected
            .as_ref()
            .and_then(|path| self.entries.iter().find(|e| &e.path == path))
            .and_then(|entry| entry.preset.as_ref().err().map(String::as_str))
    }
    fn errors(&self) -> Vec<String> {
        if let Some(error) = self.source_error() {
            vec![format!(
                "This file cannot be parsed: {error}. Fix it externally and Revert, or choose New."
            )]
        } else {
            self.draft.parse().err().unwrap_or_default()
        }
    }
    fn destination(&self) -> PathBuf {
        if self.loaded_name == self.draft.text("name")
            && let Some(path) = &self.selected
        {
            return path.clone();
        }
        self.dir
            .join(format!("{}.json", slug(self.draft.text("name"))))
    }
    fn dirty(&self) -> bool {
        self.draft.values != self.baseline.values
    }
    fn save(&mut self, confirm: bool) -> Result<PathBuf, String> {
        if let Some(error) = self.source_error() {
            return Err(format!("Cannot save the invalid source: {error}"));
        }
        let preset = self.draft.parse().map_err(|e| e.join("; "))?;
        let path = self.destination();
        let current = match std::fs::read(&path) {
            Ok(b) => Some(b),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.to_string()),
        };
        let own = self.selected.as_ref() == Some(&path) && current == self.original;
        if let Some(bytes) = &current {
            let confirmed = confirm
                && self
                    .overwrite
                    .as_ref()
                    .is_some_and(|(p, b)| p == &path && b == bytes);
            if !own && !confirmed {
                self.overwrite = Some((path.clone(), bytes.clone()));
                self.rebuild = true;
                return Err(format!(
                    "{} exists or changed since opening. Replace confirms these exact contents; Cancel keeps them.",
                    path.display()
                ));
            }
        }
        std::fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        let text = format!(
            "{}\n",
            serde_json::to_string_pretty(&preset).map_err(|e| e.to_string())?
        );
        atomic_write(&path, text.as_bytes(), current.is_some())
            .map_err(|e| format!("Cannot save {}: {e}", path.display()))?;
        self.entries = entries(&self.dir);
        if let Some(index) = self.entries.iter().position(|e| e.path == path) {
            self.select(index);
        }
        self.message = format!("Saved {}", path.display());
        Ok(path)
    }
}
fn atomic_write(path: &Path, bytes: &[u8], replace: bool) -> std::io::Result<()> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let temp = path.with_file_name(format!(
        ".{}.{}.{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        if replace {
            std::fs::rename(&temp, path)
        } else {
            std::fs::hard_link(&temp, path).and_then(|()| std::fs::remove_file(&temp))
        }
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

#[derive(Component, Clone, Debug)]
enum WorldAction {
    Select(usize),
    New,
    Duplicate,
    Revert,
    Save,
    Replace,
    Cancel,
    Page(usize),
    Toggle(String),
    Detection,
    Discard,
}
#[derive(Component)]
struct EditorRoot;
#[derive(Component)]
struct Field(String);
#[derive(Component)]
struct Status;
#[derive(Component)]
struct Destination;
#[derive(Component)]
struct SaveButton;
#[derive(Component)]
struct ToggleLabel(String);
#[derive(Component)]
struct DetectionLabel;

pub struct WorldsPlugin;
impl Plugin for WorldsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(Screen::Worlds), open).add_systems(
            Update,
            (sync_fields, rebuild, refresh)
                .chain()
                .run_if(in_state(Screen::Worlds)),
        );
    }
}
fn open(mut commands: Commands, settings: Res<AppSettings>) {
    commands.insert_resource(WorldEditor::new(settings.0.worlds_dir()));
}
fn activate(event: On<Activate>, actions: Query<&WorldAction>, mut commands: Commands) {
    if let Ok(action) = actions.get(event.entity) {
        let action = action.clone();
        commands.queue(move |world: &mut World| apply(world, action));
    }
}
fn apply(world: &mut World, action: WorldAction) {
    // Read the active fields before navigation/save even when Activate arrives before Update.
    let values: Vec<_> = world
        .query::<(&Field, &EditableText)>()
        .iter(world)
        .map(|(f, t)| (f.0.clone(), t.value().to_string()))
        .collect();
    let mut editor = world.resource_mut::<WorldEditor>();
    for (key, value) in values {
        editor.draft.values.insert(key, value);
    }
    if matches!(
        action,
        WorldAction::Select(_) | WorldAction::New | WorldAction::Duplicate
    ) && editor.dirty()
    {
        editor.discard = Some(action);
        editor.message =
            "Unsaved changes. Discard edits to continue, or Cancel to keep editing.".into();
        editor.rebuild = true;
        return;
    }
    perform(&mut editor, action);
}
fn perform(editor: &mut WorldEditor, action: WorldAction) {
    match action {
        WorldAction::Select(index) => editor.select(index),
        WorldAction::New => {
            let mut preset = worlds::plain_preset();
            preset.name = "New world".into();
            editor.draft = Draft::new(&preset);
            editor.baseline = editor.draft.clone();
            editor.selected = None;
            editor.original = None;
            editor.loaded_name.clear();
            editor.message = "New world; choose a name before saving.".into();
            editor.page = 0;
            editor.rebuild = true;
        }
        WorldAction::Duplicate => {
            if editor.source_error().is_some() {
                editor.message = "Choose a readable preset before duplicating, or use New.".into();
                return;
            }
            editor
                .draft
                .values
                .entry("name".into())
                .and_modify(|n| n.push_str(" copy"));
            editor.baseline = editor.draft.clone();
            editor.selected = None;
            editor.original = None;
            editor.loaded_name.clear();
            editor.message = "Duplicate; saving creates a separate preset.".into();
            editor.page = 0;
            editor.rebuild = true;
        }
        WorldAction::Revert => {
            if let Some(path) = editor.selected.clone() {
                editor.entries = entries(&editor.dir);
                if let Some(index) = editor.entries.iter().position(|e| e.path == path) {
                    editor.select(index);
                }
            } else {
                editor.draft = editor.baseline.clone();
                editor.rebuild = true;
            }
            editor.message = "Reverted to the starting preset.".into();
        }
        WorldAction::Save | WorldAction::Replace => {
            if let Err(e) = editor.save(matches!(action, WorldAction::Replace)) {
                editor.message = e;
            }
        }
        WorldAction::Cancel => {
            editor.overwrite = None;
            editor.discard = None;
            editor.message = String::new();
            editor.rebuild = true;
        }
        WorldAction::Page(page) => {
            editor.page = page.min(PAGES.len() - 1);
            editor.rebuild = true;
        }
        WorldAction::Toggle(key) => {
            let next = editor.draft.text(&key) != "true";
            editor.draft.values.insert(key, next.to_string());
            editor.message = "Unsaved changes".into();
            editor.overwrite = None;
        }
        WorldAction::Detection => {
            let next = if editor.draft.text("detection") == "automatic" {
                "per_roll"
            } else {
                "automatic"
            };
            editor.draft.values.insert("detection".into(), next.into());
            editor.message = "Unsaved changes".into();
        }
        WorldAction::Discard => {
            if let Some(action) = editor.discard.take() {
                perform(editor, action);
            }
        }
    }
}
fn button(p: &mut ChildSpawnerCommands, text: &str, action: WorldAction, tab: i32) -> Entity {
    let id = ui::small_button(p, text, true, tab, action);
    p.commands().entity(id).observe(activate);
    id
}
fn toggle(p: &mut ChildSpawnerCommands, draft: &Draft, key: &str, name: &str, tab: i32) {
    let id = button(
        p,
        &format!(
            "[{}] {name}",
            if draft.text(key) == "true" { "x" } else { " " }
        ),
        WorldAction::Toggle(key.into()),
        tab,
    );
    p.commands().entity(id).insert(ToggleLabel(name.into()));
}
fn field(
    p: &mut ChildSpawnerCommands,
    draft: &Draft,
    key: &str,
    label: &str,
    tab: i32,
    multiline: bool,
) {
    p.spawn(ui::label(label, 13.0, ui::TEXT));
    let mut text = EditableText {
        allow_newlines: multiline,
        visible_lines: Some(if multiline { 3.0 } else { 1.0 }),
        ..EditableText::new(draft.text(key))
    };
    text.pending_edits.clear();
    text.queue_edit(TextEdit::TextStart(false));
    let id = p
        .spawn((Field(key.into()), ui::text_field(draft.text(key), tab)))
        .id();
    p.commands().entity(id).insert((
        text,
        if multiline {
            TextLayout::default()
        } else {
            TextLayout::no_wrap()
        },
        Node {
            width: percent(100),
            min_height: px(if multiline { 68 } else { 36 }),
            padding: UiRect::all(px(8)),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(4)),
            flex_shrink: 0.0,
            ..default()
        },
    ));
}
fn sync_fields(
    fields: Query<(&Field, &EditableText), Changed<EditableText>>,
    mut editor: ResMut<WorldEditor>,
) {
    for (key, text) in &fields {
        let value = text.value().to_string();
        if editor.draft.text(&key.0) != value {
            editor.draft.values.insert(key.0.clone(), value);
            editor.message = "Unsaved changes".into();
            editor.overwrite = None;
        }
    }
}
fn rebuild(
    mut commands: Commands,
    mut editor: ResMut<WorldEditor>,
    roots: Query<Entity, With<EditorRoot>>,
) {
    if !editor.rebuild {
        return;
    }
    editor.rebuild = false;
    for entity in &roots {
        commands.entity(entity).despawn();
    }
    let panel = ui::spawn_screen(
        &mut commands,
        (EditorRoot, DespawnOnExit(Screen::Worlds)),
        "Worlds",
    );
    commands.entity(panel).insert(Node {
        width: percent(96),
        max_width: px(1200),
        height: percent(94),
        padding: UiRect::all(px(22)),
        row_gap: px(10),
        flex_direction: FlexDirection::Column,
        ..default()
    });
    commands.entity(panel).with_children(|p| {
        p.spawn(ui::label(
            "World rules are copied into new worlds. Saved worlds keep their original rules.",
            13.0,
            ui::DIM,
        ));
        p.spawn(Node {
            flex_grow: 1.0,
            min_height: px(0),
            column_gap: px(22),
            ..default()
        })
        .with_children(|body| {
            body.spawn((
                Node {
                    width: px(245),
                    flex_shrink: 0.0,
                    flex_direction: FlexDirection::Column,
                    row_gap: px(8),
                    overflow: Overflow::scroll_y(),
                    ..default()
                },
                ScrollArea,
            ))
            .with_children(|list| {
                for (i, entry) in editor.entries.iter().enumerate() {
                    let name = entry
                        .preset
                        .as_ref()
                        .map(|p| p.name.clone())
                        .unwrap_or_else(|_| {
                            entry
                                .path
                                .file_stem()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .into_owned()
                        });
                    let detail = match &entry.preset {
                        Ok(p) => worlds::validate(p)
                            .map(|()| {
                                format!(
                                    "ID: {}",
                                    worlds::preset_id(&entry.path).unwrap_or_default()
                                )
                            })
                            .unwrap_or_else(|e| format!("Invalid: {}", e.join("; "))),
                        Err(e) => format!("Invalid: {e}"),
                    };
                    let selected = editor.selected.as_ref() == Some(&entry.path);
                    let id = ui::button(
                        list,
                        &format!("{}{name}", if selected { "> " } else { "" }),
                        Some(&detail),
                        true,
                        i as i32,
                        WorldAction::Select(i),
                    );
                    list.commands().entity(id).observe(activate);
                }
                ui::row(list, |r| {
                    button(r, "New", WorldAction::New, 80);
                    button(r, "Duplicate", WorldAction::Duplicate, 81);
                });
            });
            body.spawn(Node {
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                min_width: px(0),
                min_height: px(0),
                row_gap: px(10),
                ..default()
            })
            .with_children(|form| {
                form.spawn(Node {
                    flex_wrap: FlexWrap::Wrap,
                    column_gap: px(6),
                    row_gap: px(6),
                    flex_shrink: 0.0,
                    ..default()
                })
                .with_children(|r| {
                    for (i, name) in PAGES.iter().enumerate() {
                        button(
                            r,
                            &format!("{}{name}", if i == editor.page { "> " } else { "" }),
                            WorldAction::Page(i),
                            100 + i as i32,
                        );
                    }
                });
                form.spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: px(8),
                        overflow: Overflow::scroll_y(),
                        flex_grow: 1.0,
                        min_height: px(0),
                        padding: UiRect::right(px(10)),
                        ..default()
                    },
                    ScrollArea,
                ))
                .with_children(|fields| page_fields(fields, &editor));
            });
        });
        p.spawn((Destination, ui::label("", 12.0, ui::DIM)));
        p.spawn((Status, ui::label("", 13.0, ui::NOTE)));
        if editor.discard.is_some() {
            ui::row(p, |r| {
                button(r, "Discard edits", WorldAction::Discard, 900);
                button(r, "Cancel", WorldAction::Cancel, 901);
            });
        }
        if editor.overwrite.is_some() {
            ui::row(p, |r| {
                button(r, "Replace file", WorldAction::Replace, 902);
                button(r, "Cancel", WorldAction::Cancel, 903);
            });
        }
        ui::row(p, |r| {
            let save = button(r, "Save world", WorldAction::Save, 910);
            r.commands().entity(save).insert(SaveButton);
            button(r, "Revert", WorldAction::Revert, 911);
            crate::screens::button(r, "Back", None, true, 912, MenuAction::Go(Screen::Main));
        });
    });
}
fn page_fields(p: &mut ChildSpawnerCommands, editor: &WorldEditor) {
    if let Some(error) = editor.source_error() {
        p.spawn(ui::label(format!("Cannot parse this file: {error}\nFix it externally and Revert to reload, or choose New to create a preset."),14.0,ui::ERROR));
        return;
    }
    let d = &editor.draft;
    match editor.page {
        0 => {
            field(
                p,
                d,
                "name",
                "Name (renaming creates a separate file)",
                200,
                false,
            );
            field(p, d, "description", "Description", 201, true);
            p.spawn(ui::label("Normal edits retain the existing file and preset ID. New, duplicate and renamed presets use the name as their file name.",12.0,ui::DIM));
        }
        1..=4 => {
            let need = NEEDS[editor.page - 1];
            toggle(p, d, &format!("needs.{need}.on"), "Enabled", 200);
            p.spawn(ui::label("Need badness rises from 0 (full) to 100000 (depleted). Thresholds require low < out.",12.0,ui::DIM));
            for (i, (key, label)) in [
                ("rise_milli_per_tick", "Rise per tick (milli, 0..1000)"),
                ("low_milli", "Low threshold (milli, 0..100000)"),
                ("out_milli", "Out threshold (milli, 0..100000)"),
                ("low_speed_pct", "Speed while low (percent, 0..100)"),
            ]
            .iter()
            .enumerate()
            {
                field(
                    p,
                    d,
                    &format!("needs.{need}.{key}"),
                    label,
                    201 + i as i32,
                    false,
                );
            }
        }
        5 => {
            toggle(p, d, "day_night.on", "Day / night cycle", 200);
            field(
                p,
                d,
                "day_night.cycle_ticks",
                "Cycle length (ticks, 1200..144000)",
                201,
                false,
            );
            field(
                p,
                d,
                "day_night.night_vision_pct",
                "Night vision (percent, 0..100)",
                202,
                false,
            );
            let id = button(
                p,
                &format!("Detection: {}", d.text("detection")),
                WorldAction::Detection,
                203,
            );
            p.commands().entity(id).insert(DetectionLabel);
            field(
                p,
                d,
                "perception.forget_after_ticks",
                "Forget observations after (ticks, 200..12000)",
                204,
                false,
            );
            field(
                p,
                d,
                "perception.memory_capacity",
                "Perception memory capacity (16..512)",
                205,
                false,
            );
        }
        _ => {
            for (i, (key, label)) in [
                ("crafting", "Crafting"),
                ("vehicles", "Vehicles"),
                ("locked_doors", "Locked doors"),
                ("progression", "Progression"),
                ("map", "Map landmarks"),
            ]
            .iter()
            .enumerate()
            {
                toggle(p, d, &format!("systems.{key}"), label, 200 + i as i32);
            }
            field(
                p,
                d,
                "inhabitants.workers",
                "Worker inhabitants (0..8)",
                205,
                false,
            );
            field(
                p,
                d,
                "inhabitants.skeletons",
                "Skeleton inhabitants (0..8)",
                206,
                false,
            );
        }
    }
}
type StatusFilter = Or<(With<Destination>, With<Status>)>;
type ButtonTextFilter = (Without<Destination>, Without<Status>);

fn refresh(
    mut commands: Commands,
    editor: Res<WorldEditor>,
    mut labels: Query<(&mut Text, &mut TextColor, Has<Destination>), StatusFilter>,
    saves: Query<Entity, With<SaveButton>>,
    toggles: Query<(&Children, &WorldAction, &ToggleLabel)>,
    detection: Query<&Children, With<DetectionLabel>>,
    mut button_text: Query<&mut Text, ButtonTextFilter>,
) {
    let errors = editor.errors();
    for (mut text, mut color, destination) in &mut labels {
        color.0 = if destination {
            ui::DIM
        } else if !errors.is_empty() {
            ui::ERROR
        } else {
            ui::NOTE
        };
        text.0 = if destination {
            format!("Save to: {}", editor.destination().display())
        } else if errors.is_empty() {
            if editor.message.is_empty() {
                "Valid world. Ready to save.".into()
            } else {
                editor.message.clone()
            }
        } else {
            format!(
                "{}{}",
                if editor.message.is_empty() {
                    String::new()
                } else {
                    format!("{}\n", editor.message)
                },
                errors.join("; ")
            )
        };
    }
    for entity in &saves {
        if errors.is_empty() {
            commands.entity(entity).remove::<InteractionDisabled>();
        } else {
            commands.entity(entity).insert(InteractionDisabled);
        }
    }
    for (children, action, label) in &toggles {
        if let WorldAction::Toggle(key) = action
            && let Some(child) = children.first()
            && let Ok(mut text) = button_text.get_mut(*child)
        {
            text.0 = format!(
                "[{}] {}",
                if editor.draft.text(key) == "true" {
                    "x"
                } else {
                    " "
                },
                label.0
            );
        }
    }
    for children in &detection {
        if let Some(child) = children.first()
            && let Ok(mut text) = button_text.get_mut(*child)
        {
            text.0 = format!("Detection: {}", editor.draft.text("detection"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screens::tests::{app, tmp};
    #[test]
    fn all_world_fields_round_trip_and_revalidate() {
        for (id, _) in worlds::BUILTIN_PRESETS {
            let preset = worlds::builtin_preset(id).unwrap();
            assert_eq!(Draft::new(&preset).parse().unwrap(), preset);
        }
        let mut draft = Draft::new(&worlds::plain_preset());
        draft
            .values
            .insert("needs.battery.low_speed_pct".into(), "101".into());
        assert!(
            draft
                .parse()
                .unwrap_err()
                .iter()
                .any(|e| e.contains("low_speed_pct"))
        );
        draft
            .values
            .insert("needs.battery.low_speed_pct".into(), "-".into());
        assert!(draft.parse().is_err());
    }
    #[test]
    fn world_save_retains_identity_and_guards_stale_collision() {
        let dir = tmp("world-save");
        let mut editor = WorldEditor::new(dir.clone());
        let original = editor.destination();
        editor
            .draft
            .values
            .insert("description".into(), "Changed rules".into());
        assert_eq!(editor.save(false).unwrap(), original);
        assert_eq!(
            worlds::find_preset(&dir, &worlds::preset_id(&original).unwrap())
                .unwrap()
                .description,
            "Changed rules"
        );
        let mut external = std::fs::read(&original).unwrap();
        external.push(b' ');
        std::fs::write(&original, &external).unwrap();
        assert!(editor.save(false).is_err());
        let mut raced = external.clone();
        raced.push(b' ');
        std::fs::write(&original, &raced).unwrap();
        assert!(editor.save(true).is_err());
        assert_eq!(std::fs::read(&original).unwrap(), raced);
        editor.save(true).unwrap();
        let original_name = editor.draft.text("name").to_owned();
        perform(&mut editor, WorldAction::Duplicate);
        editor.draft.values.insert("name".into(), original_name);
        let destination = editor.destination();
        std::fs::write(&destination, b"occupied").unwrap();
        assert!(editor.save(false).is_err());
        assert_eq!(std::fs::read(destination).unwrap(), b"occupied");
        let _ = std::fs::remove_dir_all(dir);
    }
    #[test]
    fn new_atomic_publication_never_replaces_a_racing_file() {
        let dir = tmp("world-race");
        let path = dir.join("race.json");
        std::fs::write(&path, b"first").unwrap();
        assert!(atomic_write(&path, b"second", false).is_err());
        assert_eq!(std::fs::read(path).unwrap(), b"first");
        let _ = std::fs::remove_dir_all(dir);
    }
    #[test]
    fn worlds_screen_lists_invalid_reasons_and_pages_without_replacing_typed_field() {
        let dir = tmp("world-ui");
        let mut app = app(&dir, &dir.join("agents"));
        let presets = app.world().resource::<AppSettings>().0.worlds_dir();
        worlds::install_builtins(&presets).unwrap();
        std::fs::write(presets.join("invalid.json"), "not JSON").unwrap();
        app.world_mut()
            .resource_mut::<NextState<Screen>>()
            .set(Screen::Worlds);
        app.update();
        app.update();
        assert_eq!(app.world().resource::<WorldEditor>().entries.len(), 4);
        let entity = app
            .world_mut()
            .query::<(Entity, &Field)>()
            .iter(app.world())
            .find(|(_, f)| f.0 == "name")
            .unwrap()
            .0;
        app.world_mut()
            .get_mut::<EditableText>(entity)
            .unwrap()
            .editor_mut()
            .set_text("");
        app.update();
        assert!(app.world().get_entity(entity).is_ok());
        assert!(
            app.world_mut()
                .query_filtered::<Entity, (With<SaveButton>, With<InteractionDisabled>)>()
                .iter(app.world())
                .next()
                .is_some()
        );
        for page in 1..PAGES.len() {
            apply(app.world_mut(), WorldAction::Page(page));
            app.update();
            assert!(
                !app.world_mut()
                    .query::<&Field>()
                    .iter(app.world())
                    .collect::<Vec<_>>()
                    .is_empty()
            );
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}
