//! Standalone actor preset editor (plan Task 14). Files and draft UI state
//! belong to the host; the validation world is frozen and never ticks.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use bevy::input_focus::InputFocus;
use bevy::input_focus::tab_navigation::{TabGroup, TabIndex, TabNavigationPlugin};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::text::{EditableText, TextCursorStyle, TextEdit};
use bevy::ui::{InteractionDisabled, Pressed};
use bevy::ui_widgets::{Activate, Button as WidgetButton, ScrollArea, TextInput};
use testbed_content::actors::{
    self, ActorPreset, AttributesSpec, Body, PerceptionSpec, SensorSpec, SheetSpec,
};
use testbed_content::explorer::home::slug;
use testbed_view::{ModelPreview, ModelPreviewPlugin, ModelPreviewSettings};

const PANEL: Color = Color::srgb(0.08, 0.09, 0.11);
const FIELD: Color = Color::srgb(0.04, 0.05, 0.06);
const TEXT: Color = Color::srgb(0.92, 0.94, 0.91);
const DIM: Color = Color::srgb(0.65, 0.69, 0.73);
const NOTE: Color = Color::srgb(0.95, 0.80, 0.45);
const ERROR: Color = Color::srgb(0.99, 0.62, 0.57);
const OK: Color = Color::srgb(0.60, 0.88, 0.62);
const BORDER: Color = Color::srgb(0.35, 0.40, 0.48);
const BUTTON: Color = Color::srgb(0.20, 0.24, 0.30);
const ACTIVE: Color = Color::srgb(0.36, 0.30, 0.15);
const HOVER: Color = Color::srgb(0.27, 0.32, 0.40);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Page {
    Identity,
    Sheet,
    Actions,
    Sensors,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum FieldKey {
    Name,
    Description,
    Speed,
    Carry,
    St,
    Dx,
    Iq,
    Ht,
    Unspent,
    Detection,
    SkillName(usize),
    SkillPoints(usize),
    TraitName(usize),
    TraitLevel(usize),
    Sensor(usize, String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SensorKind {
    Vision,
    Hearing,
    Lidar,
    Camera,
    Radar,
    Reach,
}

impl SensorKind {
    const ALL: [Self; 6] = [
        Self::Vision,
        Self::Hearing,
        Self::Lidar,
        Self::Camera,
        Self::Radar,
        Self::Reach,
    ];
    fn name(self) -> &'static str {
        match self {
            Self::Vision => "vision",
            Self::Hearing => "hearing",
            Self::Lidar => "lidar",
            Self::Camera => "camera",
            Self::Radar => "radar",
            Self::Reach => "reach",
        }
    }
    fn initial(self) -> SensorSpec {
        match self {
            Self::Vision => SensorSpec::Vision {
                range_mm: 15_000,
                half_fov_deg: 60,
            },
            Self::Hearing => SensorSpec::Hearing { radius_mm: 10_000 },
            Self::Lidar => SensorSpec::Lidar {
                range_mm: 20_000,
                fov_deg: 360,
                beams: 360,
                noise_ppm: 10_000,
            },
            Self::Camera => SensorSpec::Camera {
                range_mm: 15_000,
                half_fov_deg: 35,
            },
            Self::Radar => SensorSpec::Radar {
                range_mm: 30_000,
                half_fov_deg: 60,
                range_res_mm: 500,
                bearing_res_mdeg: 2000,
            },
            Self::Reach => SensorSpec::Reach { radius_mm: 1500 },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SensorDraft {
    kind: String,
    fields: BTreeMap<String, String>,
}

impl SensorDraft {
    fn from_spec(sensor: SensorSpec) -> Self {
        let serde_json::Value::Object(mut values) =
            serde_json::to_value(sensor).expect("sensor serializes")
        else {
            unreachable!()
        };
        let kind = values
            .remove("type")
            .and_then(|v| v.as_str().map(str::to_owned))
            .expect("sensor type");
        Self {
            kind,
            fields: values
                .into_iter()
                .map(|(k, v)| (k, v.to_string()))
                .collect(),
        }
    }
    fn parse(&self, index: usize, errors: &mut Vec<String>) -> Option<SensorSpec> {
        let mut value = serde_json::Map::new();
        value.insert("type".into(), self.kind.clone().into());
        for (name, text) in &self.fields {
            match text.trim().parse::<i64>() {
                Ok(n) => {
                    value.insert(name.clone(), n.into());
                }
                Err(_) => errors.push(format!("Sensor {} {name}: enter a whole number", index + 1)),
            }
        }
        match serde_json::from_value(serde_json::Value::Object(value)) {
            Ok(sensor) => Some(sensor),
            Err(e) => {
                errors.push(format!("Sensor {} ({}): {e}", index + 1, self.kind));
                None
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Draft {
    fields: BTreeMap<FieldKey, String>,
    body: Body,
    model: String,
    skills: Vec<(String, String)>,
    traits: Vec<(String, String)>,
    sensors: Vec<SensorDraft>,
    actions: BTreeSet<String>,
}

impl Draft {
    fn from_preset(p: &ActorPreset) -> Self {
        let a = p.sheet.attributes;
        Self {
            fields: BTreeMap::from([
                (FieldKey::Name, p.name.clone()),
                (FieldKey::Description, p.description.clone()),
                (FieldKey::Speed, p.speed_mm_per_tick.to_string()),
                (FieldKey::Carry, p.carry_capacity.to_string()),
                (FieldKey::St, a.st.to_string()),
                (FieldKey::Dx, a.dx.to_string()),
                (FieldKey::Iq, a.iq.to_string()),
                (FieldKey::Ht, a.ht.to_string()),
                (FieldKey::Unspent, p.sheet.unspent.to_string()),
                (
                    FieldKey::Detection,
                    p.perception.detection.clone().unwrap_or_default(),
                ),
            ]),
            body: p.body,
            model: p.model.clone(),
            actions: p.actions.clone(),
            skills: p
                .sheet
                .skills
                .iter()
                .map(|(name, n)| (name.clone(), n.to_string()))
                .collect(),
            traits: p
                .sheet
                .traits
                .iter()
                .map(|(name, n)| (name.clone(), n.to_string()))
                .collect(),
            sensors: p
                .sensors
                .iter()
                .copied()
                .map(SensorDraft::from_spec)
                .collect(),
        }
    }
    fn text(&self, key: &FieldKey) -> String {
        match key {
            FieldKey::SkillName(i) => self.skills.get(*i).map(|v| v.0.clone()),
            FieldKey::SkillPoints(i) => self.skills.get(*i).map(|v| v.1.clone()),
            FieldKey::TraitName(i) => self.traits.get(*i).map(|v| v.0.clone()),
            FieldKey::TraitLevel(i) => self.traits.get(*i).map(|v| v.1.clone()),
            FieldKey::Sensor(i, k) => self.sensors.get(*i).and_then(|s| s.fields.get(k).cloned()),
            _ => self.fields.get(key).cloned(),
        }
        .unwrap_or_default()
    }
    fn set_text(&mut self, key: &FieldKey, text: String) {
        match key {
            FieldKey::SkillName(i) => {
                if let Some(v) = self.skills.get_mut(*i) {
                    v.0 = text;
                }
            }
            FieldKey::SkillPoints(i) => {
                if let Some(v) = self.skills.get_mut(*i) {
                    v.1 = text;
                }
            }
            FieldKey::TraitName(i) => {
                if let Some(v) = self.traits.get_mut(*i) {
                    v.0 = text;
                }
            }
            FieldKey::TraitLevel(i) => {
                if let Some(v) = self.traits.get_mut(*i) {
                    v.1 = text;
                }
            }
            FieldKey::Sensor(i, k) => {
                if let Some(s) = self.sensors.get_mut(*i) {
                    s.fields.insert(k.clone(), text);
                }
            }
            _ => {
                self.fields.insert(key.clone(), text);
            }
        }
    }
    fn parse(&self) -> Result<ActorPreset, Vec<String>> {
        let mut errors = Vec::new();
        let mut number = |key: FieldKey, label: &str| -> i32 {
            match self.text(&key).trim().parse() {
                Ok(n) => n,
                Err(_) => {
                    errors.push(format!("{label}: enter a whole number"));
                    0
                }
            }
        };
        let attributes = AttributesSpec {
            st: number(FieldKey::St, "ST"),
            dx: number(FieldKey::Dx, "DX"),
            iq: number(FieldKey::Iq, "IQ"),
            ht: number(FieldKey::Ht, "HT"),
        };
        let unspent = number(FieldKey::Unspent, "Unspent points");
        let speed_mm_per_tick = number(FieldKey::Speed, "Speed (mm/tick)");
        let carry_capacity = match self.text(&FieldKey::Carry).trim().parse::<u32>() {
            Ok(n) => n,
            Err(_) => {
                errors.push("Carrying capacity: enter a nonnegative whole number in grams".into());
                0
            }
        };
        let mut skills = BTreeMap::new();
        for (name, n) in &self.skills {
            if name.trim().is_empty() {
                errors
                    .push("Skill name is empty; enter a registered skill or remove the row".into());
            }
            match n.trim().parse::<i32>() {
                Ok(n) => {
                    if skills.insert(name.trim().to_owned(), n).is_some() {
                        errors.push(format!("Duplicate skill: {name}"));
                    }
                }
                Err(_) => errors.push(format!("Skill {name}: enter whole points")),
            }
        }
        let mut traits = BTreeMap::new();
        for (name, n) in &self.traits {
            if name.trim().is_empty() {
                errors
                    .push("Trait name is empty; enter a registered trait or remove the row".into());
            }
            match n.trim().parse::<u8>() {
                Ok(n) => {
                    if traits.insert(name.trim().to_owned(), n).is_some() {
                        errors.push(format!("Duplicate trait: {name}"));
                    }
                }
                Err(_) => errors.push(format!("Trait {name}: level must be 0..255")),
            }
        }
        let sensors = self
            .sensors
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.parse(i, &mut errors))
            .collect();
        let detection = self.text(&FieldKey::Detection).trim().to_owned();
        let preset = ActorPreset {
            name: self.text(&FieldKey::Name),
            description: self.text(&FieldKey::Description),
            body: self.body,
            model: self.model.clone(),
            actions: self.actions.clone(),
            sensors,
            sheet: SheetSpec {
                attributes,
                skills,
                traits,
                unspent,
            },
            speed_mm_per_tick,
            carry_capacity,
            perception: PerceptionSpec {
                detection: (!detection.is_empty()).then_some(detection),
            },
        };
        if errors.is_empty() {
            Ok(preset)
        } else {
            Err(errors)
        }
    }
}

#[derive(Clone, Debug)]
struct Entry {
    path: PathBuf,
    preset: Result<ActorPreset, String>,
    errors: Vec<String>,
}

#[derive(Clone, Debug)]
struct Overwrite {
    path: PathBuf,
    expected: Vec<u8>,
}

#[derive(Resource)]
struct ActorEditor {
    dir: PathBuf,
    entries: Vec<Entry>,
    selected: Option<PathBuf>,
    draft: Option<Draft>,
    original: Option<Draft>,
    loaded_bytes: Option<Vec<u8>>,
    errors: Vec<String>,
    points: Option<i64>,
    page: Page,
    message: String,
    overwrite: Option<Overwrite>,
    requests: Vec<Action>,
    rebuild: bool,
    pending: Option<Box<Action>>,
    known_actions: Vec<String>,
}

impl ActorEditor {
    fn load(dir: PathBuf, world: &World) -> Self {
        let entries = entries(&dir, world);
        let mut editor = Self {
            dir,
            entries,
            selected: None,
            draft: None,
            original: None,
            loaded_bytes: None,
            errors: Vec::new(),
            points: None,
            page: Page::Identity,
            message: String::new(),
            overwrite: None,
            requests: Vec::new(),
            rebuild: true,
            pending: None,
            known_actions: actors::known_actions(world).into_iter().collect(),
        };
        if !editor.entries.is_empty() {
            editor.select(0, world);
        } else {
            editor.new_draft(world);
        }
        editor
    }
    fn validate(&mut self, world: &World) {
        self.errors.clear();
        self.points = None;
        self.overwrite = None;
        match self.draft.as_ref().map(Draft::parse) {
            Some(Ok(p)) => {
                self.errors = actors::validate(&p, world).err().unwrap_or_default();
                self.points = actors::sheet_point_total(&p.sheet, world).ok();
            }
            Some(Err(errors)) => self.errors = errors,
            None => self.errors.push(
                "This file cannot be parsed. Fix it in an editor, or create a new preset.".into(),
            ),
        }
    }
    fn select(&mut self, i: usize, world: &World) {
        let Some(entry) = self.entries.get(i) else {
            return;
        };
        let path = entry.path.clone();
        // The draft and its optimistic-concurrency bytes must come from
        // the same read, including a hand edit since the list was loaded.
        let (preset, bytes) = match read_editor_preset(&path) {
            Ok((bytes, preset)) => (Ok(preset), Some(bytes)),
            Err(error) => (Err(error), None),
        };
        self.selected = Some(path);
        self.loaded_bytes = bytes;
        self.draft = preset.as_ref().ok().map(Draft::from_preset);
        self.original = self.draft.clone();
        self.pending = None;
        self.message = preset.as_ref().err().cloned().unwrap_or_default();
        self.entries[i].preset = preset;
        self.validate(world);
        self.entries[i].errors = self.errors.clone();
        self.rebuild = true;
    }
    fn new_draft(&mut self, world: &World) {
        let mut p = actors::human_preset();
        p.name = "New actor".into();
        self.selected = None;
        self.loaded_bytes = None;
        self.draft = Some(Draft::from_preset(&p));
        self.original = self.draft.clone();
        self.pending = None;
        self.message = "New preset from Human. Give it a name, then Save.".into();
        self.validate(world);
        self.rebuild = true;
    }
    fn dirty(&self) -> bool {
        self.draft.is_some() && (self.selected.is_none() || self.draft != self.original)
    }
    fn destination(&self, name: &str) -> PathBuf {
        if self
            .original
            .as_ref()
            .is_some_and(|d| d.text(&FieldKey::Name) == name)
            && let Some(path) = &self.selected
        {
            return path.clone();
        }
        self.dir.join(format!("{}.json", slug(name)))
    }
    fn save(&mut self, confirm: bool, world: &World) -> Result<PathBuf, String> {
        if !self.errors.is_empty() {
            return Err("Fix the validation errors before saving.".into());
        }
        let p = self
            .draft
            .as_ref()
            .ok_or("No preset selected")?
            .parse()
            .map_err(|e| e.join("; "))?;
        actors::validate(&p, world).map_err(|e| e.join("; "))?;
        let path = self.destination(&p.name);
        let current = match std::fs::read(&path) {
            Ok(bytes) => Some(bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(format!("Cannot read {}: {e}", path.display())),
        };
        let own_unchanged = self.selected.as_ref() == Some(&path) && current == self.loaded_bytes;
        if let Some(current) = &current {
            let confirmed = confirm
                && self
                    .overwrite
                    .as_ref()
                    .is_some_and(|c| c.path == path && &c.expected == current);
            if !own_unchanged && !confirmed {
                self.overwrite = Some(Overwrite {
                    path: path.clone(),
                    expected: current.clone(),
                });
                self.message = format!(
                    "{} already exists or changed. Confirm Replace to overwrite this file; Cancel keeps it.",
                    path.display()
                );
                self.rebuild = true;
                return Err("overwrite_confirmation_required".into());
            }
        }
        std::fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        let text = format!(
            "{}\n",
            serde_json::to_string_pretty(&p).map_err(|e| e.to_string())?
        );
        atomic_preset_write(&path, text.as_bytes(), current.is_some())
            .map_err(|e| format!("Cannot save {}: {e}", path.display()))?;
        self.entries = entries(&self.dir, world);
        let i = self
            .entries
            .iter()
            .position(|e| e.path == path)
            .ok_or("Saved file did not reload")?;
        self.select(i, world);
        self.message = format!("Saved {}", path.display());
        Ok(path)
    }
}

fn entries(dir: &Path, world: &World) -> Vec<Entry> {
    actors::load_presets(dir)
        .into_iter()
        .map(|(path, preset)| {
            let errors = match &preset {
                Ok(p) => actors::validate(p, world).err().unwrap_or_default(),
                Err(e) => vec![e.clone()],
            };
            Entry {
                path,
                preset,
                errors,
            }
        })
        .collect()
}

fn read_editor_preset(path: &Path) -> Result<(Vec<u8>, ActorPreset), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    let json = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&bytes);
    let preset = serde_json::from_slice(json)
        .map_err(|e| format!("Cannot parse {}: {e}", path.display()))?;
    Ok((bytes, preset))
}

fn atomic_preset_write(path: &Path, bytes: &[u8], replace: bool) -> std::io::Result<()> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let temp = path.with_file_name(format!(
        ".{name}.{}.{}.tmp",
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

struct ValidationApp(App);

#[derive(Component, Clone, Debug)]
enum Action {
    Select(usize),
    New,
    Duplicate,
    Revert,
    Save,
    Replace,
    CancelReplace,
    Page(Page),
    Body(Body),
    Model(String),
    ToggleVerb(String),
    AddSkill,
    RemoveSkill(usize),
    AddTrait,
    RemoveTrait(usize),
    AddSensor(SensorKind),
    RemoveSensor(usize),
    ShowBody,
    ShowRanges,
    Spin,
    DiscardPending,
    KeepDraft,
    Close,
}

#[derive(Component)]
struct EditorField(FieldKey);
#[derive(Component)]
struct ListBox;
#[derive(Component)]
struct FormBox;
#[derive(Component)]
struct ValidationText;
#[derive(Component)]
struct StatusText;
#[derive(Component)]
struct BudgetText;
#[derive(Component)]
struct SaveButton;
#[derive(Component)]
struct SelectionText;
#[derive(Component)]
struct DestinationText;
#[derive(Component)]
struct EditorButton;

type EditorText = (
    &'static mut Text,
    &'static mut TextColor,
    Has<ValidationText>,
    Has<BudgetText>,
    Has<StatusText>,
    Has<SelectionText>,
    Has<DestinationText>,
);
type EditorTextFilter = Or<(
    With<ValidationText>,
    With<BudgetText>,
    With<StatusText>,
    With<SelectionText>,
    With<DestinationText>,
)>;
type EditorBoxFilter = Or<(With<ListBox>, With<FormBox>)>;
type EditorButtonState = (
    Entity,
    &'static Action,
    &'static mut BackgroundColor,
    &'static mut BorderColor,
    Option<&'static Hovered>,
    Has<Pressed>,
    Has<InteractionDisabled>,
);

/// Install the editor into a windowed app with DefaultPlugins. It owns no
/// simulation driver, input bindings or clocks belonging to a running game.
pub fn configure(app: &mut App, actors_dir: PathBuf) {
    let validator = testbed_content::new_sim_app(1, "explorer");
    let editor = ActorEditor::load(actors_dir, validator.world());
    app.insert_resource(preview(&editor))
        .insert_resource(editor)
        .insert_non_send(ValidationApp(validator));
    if !app.is_plugin_added::<TabNavigationPlugin>() {
        app.add_plugins(TabNavigationPlugin);
    }
    app.add_plugins(ModelPreviewPlugin);
    testbed_view::install_text_clipboard(app);
    app.add_message::<bevy::window::WindowCloseRequested>()
        .add_observer(on_activate)
        .add_systems(Startup, spawn)
        .add_systems(
            Update,
            (
                capture_fields,
                close_requested,
                apply_requests,
                refresh,
                paint,
                keys,
            )
                .chain()
                .before(testbed_view::ModelPreviewSystems),
        );
}

/// Select a preset before the first frame (also used by deterministic UI captures).
pub fn select_initial(app: &mut App, id: &str) -> Result<(), String> {
    let i = app
        .world()
        .resource::<ActorEditor>()
        .entries
        .iter()
        .position(|e| actors::preset_id(&e.path).as_deref() == Some(id))
        .ok_or_else(|| format!("Unknown editor preset `{id}`"))?;
    app.world_mut()
        .resource_scope(|world, mut editor: Mut<ActorEditor>| {
            editor.select(i, world.non_send::<ValidationApp>().0.world());
        });
    let p = preview(app.world().resource::<ActorEditor>());
    app.insert_resource(p);
    Ok(())
}

fn preview(editor: &ActorEditor) -> ModelPreview {
    editor
        .draft
        .as_ref()
        .map(|d| ModelPreview {
            model: d.model.clone(),
            sensors: d
                .sensors
                .iter()
                .enumerate()
                .filter_map(|(i, s)| s.parse(i, &mut Vec::new()))
                .collect(),
        })
        .unwrap_or_default()
}

// UI helpers and systems follow the same TextInput / Activate conventions
// as the menu and F7 editor; they do not create their own text event path.

fn font(size: f32) -> TextFont {
    TextFont {
        font_size: FontSize::Px(size),
        ..default()
    }
}

fn label(text: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (Text::new(text), font(size), TextColor(color))
}

fn button(p: &mut ChildSpawnerCommands, name: &str, action: Action, tab: i32) -> Entity {
    p.spawn((
        EditorButton,
        action,
        WidgetButton,
        Hovered::default(),
        TabIndex(tab),
        Node {
            padding: UiRect::axes(px(9), px(7)),
            min_height: px(32),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(4)),
            flex_shrink: 0.0,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            ..default()
        },
        BackgroundColor(BUTTON),
        BorderColor::all(BUTTON),
    ))
    .with_child(label(name, 14.0, TEXT))
    .id()
}

fn row(p: &mut ChildSpawnerCommands, children: impl FnOnce(&mut ChildSpawnerCommands)) {
    p.spawn(Node {
        flex_wrap: FlexWrap::Wrap,
        column_gap: px(6),
        row_gap: px(6),
        align_items: AlignItems::Center,
        ..default()
    })
    .with_children(children);
}

fn heading(p: &mut ChildSpawnerCommands, name: &str) {
    p.spawn((
        label(name, 16.0, TEXT),
        Node {
            margin: UiRect::top(px(14)),
            ..default()
        },
    ));
}

fn text_field(
    p: &mut ChildSpawnerCommands,
    draft: &Draft,
    key: FieldKey,
    name: &str,
    tab: i32,
    multiline: bool,
) {
    p.spawn(label(name, 12.0, DIM));
    let mut text = EditableText {
        allow_newlines: multiline,
        visible_lines: Some(if multiline { 3.0 } else { 1.0 }),
        ..EditableText::new(draft.text(&key))
    };
    // EditableText::new initially reveals its ending. A newly opened form
    // starts at the beginning; subsequent field edits retain their caret.
    text.pending_edits.clear();
    text.queue_edit(TextEdit::TextStart(false));
    p.spawn((
        EditorField(key),
        TextInput,
        text,
        TextCursorStyle::default(),
        if multiline {
            TextLayout::default()
        } else {
            TextLayout::no_wrap()
        },
        font(14.0),
        TextColor(TEXT),
        TabIndex(tab),
        Node {
            width: percent(100),
            min_width: px(0),
            min_height: px(if multiline { 68.0 } else { 36.0 }),
            padding: UiRect::all(px(7)),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(3)),
            flex_shrink: 0.0,
            ..default()
        },
        BackgroundColor(FIELD),
        BorderColor::all(BORDER),
    ));
}

fn spawn(mut commands: Commands) {
    commands.spawn((Node { width: percent(100), height: percent(100), position_type: PositionType::Absolute, ..default() },TabGroup::new(0))).with_children(|root| {
        root.spawn((Node { position_type: PositionType::Absolute, left: px(0), right: px(0), top: px(0), height: px(64), padding: UiRect::axes(px(18),px(12)), column_gap: px(14), align_items: AlignItems::Center, ..default() }, BackgroundColor(PANEL))).with_children(|top| {
            top.spawn(label("Actor editor",22.0,TEXT));
            top.spawn((SelectionText,label("",14.0,DIM),Node { flex_grow: 1.0, min_width: px(0), ..default() }));
            button(top,"Revert",Action::Revert,10001);
            let save = button(top,"Save preset",Action::Save,10000);
            top.commands().entity(save).insert(SaveButton);
            button(top,"Close",Action::Close,10002);
        });
        root.spawn((Node { position_type: PositionType::Absolute, left: px(0), top: px(64), bottom: px(64), width: px(260), padding: UiRect::all(px(16)), row_gap: px(10), flex_direction: FlexDirection::Column, ..default() },BackgroundColor(PANEL))).with_children(|left| {
            row(left,|r| { button(r,"New",Action::New,0); button(r,"Duplicate",Action::Duplicate,1); });
            left.spawn(label("PRESETS",12.0,DIM));
            left.spawn((ListBox,ScrollArea,Node { flex_direction: FlexDirection::Column, row_gap: px(7), flex_grow: 1.0, min_height: px(0), overflow: Overflow::scroll_y(), ..default() }));
        });
        root.spawn((Node { position_type: PositionType::Absolute, right: px(0), top: px(64), bottom: px(64), width: px(460), padding: UiRect::all(px(16)), row_gap: px(10), flex_direction: FlexDirection::Column, ..default() }, BackgroundColor(PANEL))).with_children(|right| {
            right.spawn((BudgetText,label("",14.0,NOTE)));
            row(right,|r| {
                for (i,(name,page)) in [("Identity",Page::Identity),("Sheet",Page::Sheet),("Actions",Page::Actions),("Sensors",Page::Sensors)].into_iter().enumerate() { button(r,name,Action::Page(page),200+i as i32); }
            });
            right.spawn((ScrollArea,Node { max_height: px(112), overflow: Overflow::scroll_y(), flex_shrink: 0.0, ..default() })).with_child((ValidationText,label("",12.0,ERROR)));
            right.spawn((FormBox,ScrollArea,Node { flex_direction: FlexDirection::Column, row_gap: px(7), flex_grow: 1.0, min_height: px(0), overflow: Overflow::scroll_y(), ..default() }));
        });
        root.spawn(Node { position_type: PositionType::Absolute, left: px(278), right: px(478), bottom: px(117), flex_wrap: FlexWrap::Wrap, column_gap: px(4), row_gap: px(4), justify_content: JustifyContent::Center, ..default() }).with_children(|models| {
            for (i,id) in actors::MODELS.iter().enumerate() {
                let name = match *id { "rogue" => "Rogue", "knight" => "Knight", "skeleton_minion" => "Minion", "skeleton_warrior" => "Warrior", "rover" => "Rover", _ => "Sentinel" };
                button(models,name,Action::Model((*id).into()),100+i as i32);
            }
        });
        root.spawn(Node { position_type: PositionType::Absolute, left: px(278), right: px(478), bottom: px(76), flex_wrap: FlexWrap::Wrap, column_gap: px(6), row_gap: px(4), justify_content: JustifyContent::Center, ..default() }).with_children(|r| {
            button(r,"Body",Action::ShowBody,160); button(r,"Sensor ranges",Action::ShowRanges,161); button(r,"Rotate",Action::Spin,162);
        });
        root.spawn((Node { position_type: PositionType::Absolute, left: px(0), right: px(0), bottom: px(0), height: px(64), padding: UiRect::axes(px(18),px(10)), flex_direction: FlexDirection::Column, row_gap: px(4), ..default() }, BackgroundColor(PANEL))).with_children(|footer| {
            footer.spawn((StatusText,label("",12.0,NOTE)));
            footer.spawn(label("Ctrl+S saves. Tab moves between controls. Wheel zooms the preview; Sensor ranges fits its senses.",11.0,DIM));
        });
    });
}

fn on_activate(event: On<Activate>, actions: Query<&Action>, mut commands: Commands) {
    if let Ok(action) = actions.get(event.entity).cloned() {
        commands.queue(move |world: &mut World| {
            world.resource_mut::<ActorEditor>().requests.push(action);
        });
    }
}

fn capture_fields(world: &mut World) {
    let changed: Vec<_> = world
        .query_filtered::<(&EditorField, &EditableText), Changed<EditableText>>()
        .iter(world)
        .map(|(k, t)| (k.0.clone(), t.value().to_string()))
        .collect();
    if changed.is_empty() {
        return;
    }
    world.resource_scope(|world, mut editor: Mut<ActorEditor>| {
        let mut edited = false;
        if let Some(draft) = &mut editor.draft {
            for (key, text) in changed {
                if draft.text(&key) != text {
                    draft.set_text(&key, text);
                    edited = true;
                }
            }
        }
        if edited {
            editor.validate(world.non_send::<ValidationApp>().0.world());
            editor.message = "Unsaved changes".into();
        }
    });
    let p = preview(world.resource::<ActorEditor>());
    world.resource_mut::<ModelPreview>().set_if_neq(p);
}

fn apply_requests(world: &mut World) {
    let requests = std::mem::take(&mut world.resource_mut::<ActorEditor>().requests);
    if requests.is_empty() {
        return;
    }
    world.resource_scope(|world,mut editor: Mut<ActorEditor>| {
        for mut action in requests {
            let discarding = if matches!(action,Action::DiscardPending) {
                let Some(pending) = editor.pending.take() else { continue; };
                action = *pending; true
            } else { false };
            if !discarding && editor.dirty() && matches!(action,Action::New | Action::Select(_) | Action::Close) {
                editor.pending = Some(Box::new(action)); editor.overwrite = None;
                editor.message = "Unsaved changes. Keep editing, or confirm Discard edits before opening another preset.".into();
                editor.rebuild = true; continue;
            }
            let validator = world.non_send::<ValidationApp>().0.world();
            let mut validate = true;
            match action {
                Action::Select(i) => editor.select(i,validator),
                Action::New => editor.new_draft(validator),
                Action::Duplicate => {
                    if let Some(draft) = &mut editor.draft {
                        let name = format!("{} copy",draft.text(&FieldKey::Name));
                        draft.set_text(&FieldKey::Name,name);
                        editor.original = editor.draft.clone(); editor.selected = None; editor.loaded_bytes = None;
                        editor.message = "Duplicated draft. Give the copy a unique name, then Save.".into();
                    }
                },
                Action::Revert => {
                    editor.entries = entries(&editor.dir,validator);
                    if let Some(i) = editor.entries.iter().position(|e| Some(&e.path) == editor.selected.as_ref()) { editor.select(i,validator); }
                    else if editor.selected.is_none() && editor.original.is_some() {
                        editor.draft = editor.original.clone(); editor.pending = None;
                        editor.message = "Reverted to this unsaved preset's starting draft.".into();
                    } else { editor.message = "The selected file is missing. The current draft was kept; Save can recreate it.".into(); }
                    if editor.selected.is_some() && editor.entries.iter().any(|e| Some(&e.path) == editor.selected.as_ref()) {
                        editor.message = "Reverted to the file on disk.".into();
                    }
                },
                Action::Save | Action::Replace => {
                    if let Err(e) = editor.save(matches!(action,Action::Replace),validator) && e != "overwrite_confirmation_required" { editor.message = e; }
                    validate = false;
                },
                Action::CancelReplace => { editor.overwrite = None; editor.message = "Replacement cancelled; the file is unchanged.".into(); validate = false; },
                Action::KeepDraft | Action::DiscardPending => { editor.pending = None; editor.message = "Kept the current draft.".into(); validate = false; },
                Action::Page(page) => { editor.page = page; validate = false; },
                Action::Close => { world.write_message(AppExit::Success); validate = false; },
                Action::Body(body) => if let Some(d) = &mut editor.draft { d.body = body; },
                Action::Model(model) => if let Some(d) = &mut editor.draft { d.model = model; },
                Action::ToggleVerb(name) => if let Some(d) = &mut editor.draft && !d.actions.remove(&name) { d.actions.insert(name); },
                Action::AddSkill => if let Some(d) = &mut editor.draft { d.skills.push((String::new(),"1".into())); },
                Action::RemoveSkill(i) => if let Some(d) = &mut editor.draft && i < d.skills.len() { d.skills.remove(i); },
                Action::AddTrait => if let Some(d) = &mut editor.draft { d.traits.push((String::new(),"1".into())); },
                Action::RemoveTrait(i) => if let Some(d) = &mut editor.draft && i < d.traits.len() { d.traits.remove(i); },
                Action::AddSensor(kind) => if let Some(d) = &mut editor.draft { d.sensors.push(SensorDraft::from_spec(kind.initial())); },
                Action::RemoveSensor(i) => if let Some(d) = &mut editor.draft && i < d.sensors.len() { d.sensors.remove(i); },
                Action::ShowBody | Action::ShowRanges | Action::Spin => {
                    let mut settings = world.resource_mut::<ModelPreviewSettings>();
                    match action { Action::ShowBody => settings.fit_sensors = false, Action::ShowRanges => { settings.fit_sensors = true; settings.show_sensors = true; }, Action::Spin => settings.spinning = !settings.spinning, _ => {} }
                    validate = false;
                },
            }
            if validate { editor.validate(world.non_send::<ValidationApp>().0.world()); }
            editor.rebuild = true;
        }
    });
    let p = preview(world.resource::<ActorEditor>());
    world.resource_mut::<ModelPreview>().set_if_neq(p);
    let field_focused = world
        .get_resource::<InputFocus>()
        .and_then(InputFocus::get)
        .is_some_and(|e| world.get::<EditorField>(e).is_some());
    if field_focused && let Some(mut focus) = world.get_resource_mut::<InputFocus>() {
        focus.clear();
    }
}

fn refresh(
    mut commands: Commands,
    mut editor: ResMut<ActorEditor>,
    boxes: Query<(Entity, Has<ListBox>), EditorBoxFilter>,
    mut texts: Query<EditorText, EditorTextFilter>,
    saves: Query<Entity, With<SaveButton>>,
) {
    for (mut text, mut color, validation, budget, status, selection, destination) in &mut texts {
        let value = if validation {
            color.set_if_neq(TextColor(if editor.errors.is_empty() { OK } else { ERROR }));
            if editor.errors.is_empty() {
                "Valid preset. Ready to save.".into()
            } else {
                editor.errors.join("\n")
            }
        } else if budget {
            editor.points.map_or_else(
                || "Character points: enter valid numbers".into(),
                |n| format!("Character points: {n} / {}", actors::PRESET_POINT_BUDGET),
            )
        } else if status {
            if editor.message.is_empty() {
                format!("Actors folder: {}", editor.dir.display())
            } else {
                editor.message.clone()
            }
        } else if selection {
            editor.draft.as_ref().map_or_else(
                || "Unreadable preset".into(),
                |d| {
                    format!(
                        "{}{}",
                        d.text(&FieldKey::Name),
                        if editor.dirty() { " *" } else { "" }
                    )
                },
            )
        } else if destination {
            editor
                .draft
                .as_ref()
                .map(|draft| {
                    editor
                        .destination(&draft.text(&FieldKey::Name))
                        .display()
                        .to_string()
                })
                .unwrap_or_default()
        } else {
            continue;
        };
        text.set_if_neq(Text::new(value));
    }
    for save in &saves {
        if editor.errors.is_empty() {
            commands.entity(save).remove::<InteractionDisabled>();
        } else {
            commands.entity(save).insert(InteractionDisabled);
        }
    }
    if !editor.rebuild {
        return;
    }
    editor.rebuild = false;
    for (id, list) in &boxes {
        if !list && (editor.pending.is_some() || editor.overwrite.is_some()) {
            commands.entity(id).insert(ScrollPosition::default());
        }
        commands.entity(id).despawn_children().with_children(|p| {
            if list {
                render_list(p, &editor);
            } else {
                render_form(p, &editor);
            }
        });
    }
}

fn render_list(p: &mut ChildSpawnerCommands, editor: &ActorEditor) {
    for (i, entry) in editor.entries.iter().enumerate() {
        let name = entry
            .preset
            .as_ref()
            .map(|p| p.name.clone())
            .unwrap_or_else(|_| {
                entry
                    .path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            });
        let b = button(p, &name, Action::Select(i), 10 + i as i32);
        p.commands().entity(b).insert(Node {
            width: percent(100),
            padding: UiRect::all(px(9)),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(4)),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::FlexStart,
            row_gap: px(4),
            flex_shrink: 0.0,
            ..default()
        });
        p.commands().entity(b).with_children(|b| {
            b.spawn(label(
                entry.path.file_name().unwrap_or_default().to_string_lossy(),
                11.0,
                DIM,
            ));
            b.spawn(label(
                if entry.errors.is_empty() {
                    "Valid"
                } else {
                    "Invalid - select for details"
                },
                11.0,
                if entry.errors.is_empty() { DIM } else { ERROR },
            ));
        });
    }
    if editor.entries.is_empty() {
        p.spawn(label("No presets yet. New starts from Human.", 13.0, DIM));
    }
}

fn render_form(p: &mut ChildSpawnerCommands, editor: &ActorEditor) {
    if editor.pending.is_some() {
        p.spawn(label("Discard unsaved edits?", 14.0, NOTE));
        row(p, |r| {
            button(r, "Discard edits", Action::DiscardPending, 208);
            button(r, "Keep editing", Action::KeepDraft, 209);
        });
    }
    if let Some(collision) = &editor.overwrite {
        p.spawn(label(
            format!(
                "Replace {}?",
                collision
                    .path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
            ),
            14.0,
            NOTE,
        ));
        p.spawn(label("This file belongs to another preset or changed since it was opened. Replacing it cannot be undone here.",12.0,DIM));
        row(p, |r| {
            button(r, "Confirm Replace", Action::Replace, 210);
            button(r, "Cancel", Action::CancelReplace, 211);
        });
    }
    let Some(draft) = &editor.draft else {
        p.spawn(label("This preset cannot be parsed. Repair the named file on disk, then Revert, or start a New preset.",14.0,ERROR));
        return;
    };
    match editor.page {
        Page::Identity => {
            text_field(p, draft, FieldKey::Name, "Name", 300, false);
            text_field(p, draft, FieldKey::Description, "Description", 301, true);
            heading(p, "Body");
            row(p, |r| {
                button(r, "Human", Action::Body(Body::Human), 302);
                button(r, "Robot", Action::Body(Body::Robot), 303);
            });
            p.spawn(label(
                format!("Model: {} (choose below the preview)", draft.model),
                12.0,
                DIM,
            ));
            heading(p, "Movement and load");
            text_field(
                p,
                draft,
                FieldKey::Speed,
                "Speed (mm per tick; 20 ticks per second)",
                304,
                false,
            );
            text_field(
                p,
                draft,
                FieldKey::Carry,
                "Carrying capacity (grams)",
                305,
                false,
            );
            heading(p, "Perception");
            text_field(
                p,
                draft,
                FieldKey::Detection,
                "Detection rule (blank uses the world's rule)",
                306,
                false,
            );
            p.spawn(label("Detection is a world setting. Only the active rule is valid. Memory span is configured by the world.",12.0,DIM));
            heading(p, "Save destination");
            p.spawn((
                DestinationText,
                label(
                    editor
                        .destination(&draft.text(&FieldKey::Name))
                        .display()
                        .to_string(),
                    11.0,
                    DIM,
                ),
            ));
            p.spawn(label("Normal edits keep this file's preset ID. A new or renamed preset uses its name as the file name; the original remains available.",12.0,DIM));
        }
        Page::Sheet => {
            p.spawn(label("Point-buy attributes and banked points", 13.0, DIM));
            for (i, (key, name)) in [
                (FieldKey::St, "ST - strength"),
                (FieldKey::Dx, "DX - dexterity"),
                (FieldKey::Iq, "IQ - intelligence"),
                (FieldKey::Ht, "HT - health"),
                (FieldKey::Unspent, "Unspent points"),
            ]
            .into_iter()
            .enumerate()
            {
                text_field(p, draft, key, name, 400 + i as i32, false);
            }
            heading(p, "Skills");
            for i in 0..draft.skills.len() {
                text_field(
                    p,
                    draft,
                    FieldKey::SkillName(i),
                    "Registered skill name",
                    500 + i as i32 * 3,
                    false,
                );
                text_field(
                    p,
                    draft,
                    FieldKey::SkillPoints(i),
                    "Points invested",
                    501 + i as i32 * 3,
                    false,
                );
                button(
                    p,
                    "Remove skill",
                    Action::RemoveSkill(i),
                    502 + i as i32 * 3,
                );
            }
            button(p, "Add skill", Action::AddSkill, 800);
            heading(p, "Traits");
            for i in 0..draft.traits.len() {
                text_field(
                    p,
                    draft,
                    FieldKey::TraitName(i),
                    "Registered trait name",
                    1000 + i as i32 * 3,
                    false,
                );
                text_field(
                    p,
                    draft,
                    FieldKey::TraitLevel(i),
                    "Trait level",
                    1001 + i as i32 * 3,
                    false,
                );
                button(
                    p,
                    "Remove trait",
                    Action::RemoveTrait(i),
                    1002 + i as i32 * 3,
                );
            }
            button(p, "Add trait", Action::AddTrait, 1400);
        }
        Page::Actions => {
            p.spawn(label("Allowed actions", 16.0, TEXT));
            p.spawn(label("Walking and basic interaction remain available. Toggle the specific verbs this actor may use.",12.0,DIM));
            let mut actions = editor.known_actions.clone();
            actions.extend(draft.actions.iter().cloned());
            actions.sort();
            actions.dedup();
            for (i, name) in actions.into_iter().enumerate() {
                button(
                    p,
                    &format!(
                        "[{}] {name}",
                        if draft.actions.contains(&name) {
                            "x"
                        } else {
                            " "
                        }
                    ),
                    Action::ToggleVerb(name),
                    1500 + i as i32,
                );
            }
        }
        Page::Sensors => {
            p.spawn(label("Add a sensor", 14.0, TEXT));
            row(p, |r| {
                for (i, kind) in SensorKind::ALL.into_iter().enumerate() {
                    button(r, kind.name(), Action::AddSensor(kind), 4000 + i as i32);
                }
            });
            p.spawn(label("Distances are millimetres. Reach is the vicinity sense; its default radius is 1500 mm (1.5 m), and 0 disables it.",12.0,DIM));
            for (i, sensor) in draft.sensors.iter().enumerate() {
                heading(p, &format!("{}  {}", i + 1, sensor.kind));
                for (j, param) in sensor.fields.keys().enumerate() {
                    let name = match param.as_str() {
                        "range_mm" => "Range (mm)",
                        "radius_mm" => {
                            if sensor.kind == "reach" {
                                "Vicinity radius (mm; 0 disables)"
                            } else {
                                "Radius (mm)"
                            }
                        }
                        "half_fov_deg" => "Half field of view (degrees)",
                        "fov_deg" => "Full field of view (degrees)",
                        "beams" => "Beam count",
                        "noise_ppm" => "Noise (parts per million)",
                        "range_res_mm" => "Range resolution (mm)",
                        "bearing_res_mdeg" => "Bearing resolution (millidegrees)",
                        _ => param,
                    };
                    text_field(
                        p,
                        draft,
                        FieldKey::Sensor(i, param.clone()),
                        name,
                        2000 + i as i32 * 10 + j as i32,
                        false,
                    );
                }
                button(
                    p,
                    "Remove sensor",
                    Action::RemoveSensor(i),
                    2008 + i as i32 * 10,
                );
            }
            if draft.sensors.is_empty() {
                p.spawn(label(
                    "No senses. Add reach, vision or a robot sensor to perceive the world.",
                    13.0,
                    NOTE,
                ));
            }
        }
    }
}

fn paint(
    editor: Res<ActorEditor>,
    settings: Res<ModelPreviewSettings>,
    focus: Res<InputFocus>,
    mut buttons: Query<EditorButtonState, With<EditorButton>>,
    children: Query<&Children>,
    mut colors: Query<&mut TextColor>,
) {
    for (entity, action, mut bg, mut border, hover, pressed, disabled) in &mut buttons {
        let active = match action {
            Action::Select(i) => editor
                .entries
                .get(*i)
                .is_some_and(|e| Some(&e.path) == editor.selected.as_ref()),
            Action::Page(page) => *page == editor.page,
            Action::Body(body) => editor.draft.as_ref().is_some_and(|d| d.body == *body),
            Action::Model(model) => editor.draft.as_ref().is_some_and(|d| d.model == *model),
            Action::ToggleVerb(name) => editor
                .draft
                .as_ref()
                .is_some_and(|d| d.actions.contains(name)),
            Action::ShowBody => !settings.fit_sensors,
            Action::ShowRanges => settings.fit_sensors,
            Action::Spin => settings.spinning,
            _ => false,
        };
        bg.set_if_neq(BackgroundColor(if disabled || pressed {
            FIELD
        } else if hover.is_some_and(Hovered::get) {
            HOVER
        } else if active {
            ACTIVE
        } else {
            BUTTON
        }));
        border.set_if_neq(BorderColor::all(if focus.get() == Some(entity) {
            NOTE
        } else if active {
            BORDER
        } else {
            BUTTON
        }));
        if let Some(child) = children.get(entity).ok().and_then(|c| c.first()).copied()
            && let Ok(mut color) = colors.get_mut(child)
        {
            color.set_if_neq(TextColor(if disabled { DIM } else { TEXT }));
        }
    }
}

fn keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut focus: ResMut<InputFocus>,
    fields: Query<(), With<EditorField>>,
    mut editor: ResMut<ActorEditor>,
) {
    if keys.just_pressed(KeyCode::Escape) && focus.get().is_some_and(|e| fields.contains(e)) {
        focus.clear();
    }
    let control = keys.pressed(KeyCode::ControlLeft)
        || keys.pressed(KeyCode::ControlRight)
        || keys.pressed(KeyCode::SuperLeft)
        || keys.pressed(KeyCode::SuperRight);
    if control {
        for (key, action) in [
            (KeyCode::KeyS, Action::Save),
            (KeyCode::KeyN, Action::New),
            (KeyCode::KeyD, Action::Duplicate),
        ] {
            if keys.just_pressed(key) {
                editor.requests.push(action);
            }
        }
    }
}

fn close_requested(
    mut requests: MessageReader<bevy::window::WindowCloseRequested>,
    windows: Query<Entity, With<bevy::window::PrimaryWindow>>,
    mut editor: ResMut<ActorEditor>,
) {
    if requests
        .read()
        .any(|request| windows.iter().any(|window| window == request.window))
    {
        editor.requests.push(Action::Close);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Temp(PathBuf);
    impl Temp {
        fn new(name: &str) -> Self {
            static SEQUENCE: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "npc-actor-editor-{name}-{}-{}",
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            assert!(self.0.starts_with(std::env::temp_dir()));
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn validator() -> App {
        testbed_content::new_sim_app(1, "explorer")
    }

    #[test]
    fn editor_lists_presets_with_validity() {
        let temp = Temp::new("list");
        let app = validator();
        actors::install_builtins(&temp.0).unwrap();
        let mut broken = actors::human_preset();
        broken.model = "not-a-model".into();
        std::fs::write(
            temp.0.join("invalid.json"),
            serde_json::to_string(&broken).unwrap(),
        )
        .unwrap();
        std::fs::write(temp.0.join("unreadable.json"), "not JSON").unwrap();
        let editor = ActorEditor::load(temp.0.clone(), app.world());
        assert_eq!(editor.entries.len(), 5);
        assert_eq!(
            editor
                .entries
                .iter()
                .filter(|e| e.errors.is_empty())
                .count(),
            3
        );
        let invalid = editor
            .entries
            .iter()
            .find(|e| e.path.ends_with("invalid.json"))
            .unwrap();
        assert!(invalid.errors.iter().any(|e| e.contains("unknown model")));
        let unreadable = editor
            .entries
            .iter()
            .find(|e| e.path.ends_with("unreadable.json"))
            .unwrap();
        assert!(unreadable.preset.is_err() && !unreadable.errors.is_empty());
    }

    #[test]
    fn new_preset_starts_from_human() {
        let temp = Temp::new("new");
        let app = validator();
        let mut editor = ActorEditor::load(temp.0.clone(), app.world());
        editor.new_draft(app.world());
        let preset = editor.draft.as_ref().unwrap().parse().unwrap();
        let human = actors::human_preset();
        assert_eq!(preset.body, Body::Human);
        assert_eq!(preset.model, human.model);
        assert_eq!(preset.sheet, human.sheet);
        assert_eq!(preset.actions, human.actions);
        assert_eq!(preset.sensors, human.sensors);
        assert!(editor.selected.is_none() && editor.dirty() && editor.errors.is_empty());
    }

    #[test]
    fn editing_a_field_revalidates() {
        let temp = Temp::new("validate");
        let app = validator();
        let mut editor = ActorEditor::load(temp.0.clone(), app.world());
        editor
            .draft
            .as_mut()
            .unwrap()
            .set_text(&FieldKey::Speed, "0".into());
        editor.validate(app.world());
        assert!(
            editor
                .errors
                .iter()
                .any(|e| e.contains("speed_mm_per_tick"))
        );
        editor
            .draft
            .as_mut()
            .unwrap()
            .set_text(&FieldKey::Speed, "60".into());
        editor.validate(app.world());
        assert!(editor.errors.is_empty());
        editor
            .draft
            .as_mut()
            .unwrap()
            .set_text(&FieldKey::St, "not a number".into());
        editor.validate(app.world());
        assert!(editor.errors.iter().any(|e| e.contains("ST")));
        editor
            .draft
            .as_mut()
            .unwrap()
            .set_text(&FieldKey::St, "10".into());
        editor
            .draft
            .as_mut()
            .unwrap()
            .set_text(&FieldKey::Unspent, i32::MAX.to_string());
        editor.validate(app.world());
        assert!(editor.points.unwrap() > i64::from(i32::MAX));
        assert!(editor.errors.iter().any(|e| e.contains("budget")));
    }

    #[test]
    fn every_sensor_field_and_reach_are_editable() {
        let mut draft = Draft::from_preset(&actors::human_preset());
        draft.sensors = SensorKind::ALL
            .into_iter()
            .map(|k| SensorDraft::from_spec(k.initial()))
            .collect();
        let preset = draft.parse().unwrap();
        assert_eq!(preset.sensors.len(), 6);
        draft.set_text(&FieldKey::Sensor(5, "radius_mm".into()), "2400".into());
        assert_eq!(draft.parse().unwrap().reach_mm(), Some(2400));
        draft.set_text(&FieldKey::Sensor(2, "beams".into()), "-1".into());
        assert!(
            draft
                .parse()
                .unwrap_err()
                .iter()
                .any(|e| e.contains("Sensor 3"))
        );
    }

    #[test]
    fn save_writes_the_file_and_keeps_the_presets_identity() {
        let temp = Temp::new("save");
        let app = validator();
        let mut editor = ActorEditor::load(temp.0.clone(), app.world());
        let human_index = editor
            .entries
            .iter()
            .position(|e| e.path.ends_with("human.json"))
            .unwrap();
        editor.select(human_index, app.world());
        editor
            .draft
            .as_mut()
            .unwrap()
            .set_text(&FieldKey::Speed, "61".into());
        editor.validate(app.world());
        let existing = editor.save(false, app.world()).unwrap();
        assert_eq!(
            existing,
            temp.0.join("human.json"),
            "stat edits retain the selected preset ID"
        );
        editor.new_draft(app.world());
        editor
            .draft
            .as_mut()
            .unwrap()
            .set_text(&FieldKey::Name, "Ada scout".into());
        editor.validate(app.world());
        let expected = editor.draft.as_ref().unwrap().parse().unwrap();
        let path = editor.save(false, app.world()).unwrap();
        assert_eq!(path, temp.0.join("ada-scout.json"));
        let actual: ActorPreset = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(actual, expected);
        assert!(!editor.dirty());
        assert!(
            std::fs::read_dir(&temp.0).unwrap().all(|e| !e
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp"))
        );
    }

    #[test]
    fn selecting_a_hand_edited_file_uses_one_snapshot_for_draft_and_version() {
        let temp = Temp::new("select-edit");
        let app = validator();
        let mut editor = ActorEditor::load(temp.0.clone(), app.world());
        let i = editor
            .entries
            .iter()
            .position(|e| e.path.ends_with("human.json"))
            .unwrap();
        let mut updated = actors::human_preset();
        updated.description = "A hand edit after the list was loaded".into();
        let mut bytes = b"\xef\xbb\xbf".to_vec();
        bytes.extend(serde_json::to_vec(&updated).unwrap());
        std::fs::write(temp.0.join("human.json"), &bytes).unwrap();
        editor.select(i, app.world());
        assert_eq!(editor.loaded_bytes.as_ref().unwrap(), &bytes);
        assert_eq!(
            editor.draft.as_ref().unwrap().text(&FieldKey::Description),
            updated.description
        );
        editor
            .draft
            .as_mut()
            .unwrap()
            .set_text(&FieldKey::Speed, "63".into());
        editor.validate(app.world());
        editor.save(false, app.world()).unwrap();
        let saved: ActorPreset =
            serde_json::from_slice(&std::fs::read(temp.0.join("human.json")).unwrap()).unwrap();
        assert_eq!(saved.description, updated.description);
        assert_eq!(saved.speed_mm_per_tick, 63);
    }

    #[test]
    fn save_refuses_slug_collision_without_confirm() {
        let temp = Temp::new("collision");
        let app = validator();
        let mut editor = ActorEditor::load(temp.0.clone(), app.world());
        editor.new_draft(app.world());
        editor
            .draft
            .as_mut()
            .unwrap()
            .set_text(&FieldKey::Name, "Rover".into());
        editor.validate(app.world());
        let path = temp.0.join("rover.json");
        let before = std::fs::read(&path).unwrap();
        assert_eq!(
            editor.save(false, app.world()).unwrap_err(),
            "overwrite_confirmation_required"
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
        // A hand edit after confirmation was offered requires another confirmation.
        let mut changed = actors::builtin_preset("rover").unwrap();
        changed.description.push_str(" Hand edit.");
        let changed = serde_json::to_vec(&changed).unwrap();
        std::fs::write(&path, &changed).unwrap();
        assert_eq!(
            editor.save(true, app.world()).unwrap_err(),
            "overwrite_confirmation_required"
        );
        assert_eq!(std::fs::read(&path).unwrap(), changed);
        editor.save(true, app.world()).unwrap();
        let saved: ActorPreset = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(
            saved.body,
            Body::Human,
            "the explicit second confirmation replaces the robot preset"
        );
    }

    #[test]
    fn new_file_publication_refuses_a_racing_collision() {
        let temp = Temp::new("create-only");
        let path = temp.0.join("actor.json");
        std::fs::write(&path, b"keep me").unwrap();
        assert!(atomic_preset_write(&path, b"replacement", false).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"keep me");
    }

    fn editor_world(temp: &Temp) -> World {
        let validation = validator();
        let editor = ActorEditor::load(temp.0.clone(), validation.world());
        let mut world = World::new();
        world.insert_resource(preview(&editor));
        world.insert_resource(ModelPreviewSettings::default());
        world.insert_resource(editor);
        world.insert_non_send(ValidationApp(validation));
        world
    }

    #[test]
    fn model_picker_changes_the_preview() {
        let temp = Temp::new("preview");
        let mut world = editor_world(&temp);
        world
            .resource_mut::<ActorEditor>()
            .requests
            .push(Action::Model("rover".into()));
        apply_requests(&mut world);
        assert_eq!(world.resource::<ModelPreview>().model, "rover");
        world
            .resource_mut::<ActorEditor>()
            .requests
            .push(Action::Model("sentinel".into()));
        apply_requests(&mut world);
        assert_eq!(world.resource::<ModelPreview>().model, "sentinel");
    }

    #[test]
    fn revert_reloads_saved_presets_and_restores_unsaved_duplicate_and_new_baselines() {
        let temp = Temp::new("revert");
        let mut world = editor_world(&temp);
        let saved_path = world.resource::<ActorEditor>().selected.clone();
        let saved_speed = world
            .resource::<ActorEditor>()
            .draft
            .as_ref()
            .unwrap()
            .text(&FieldKey::Speed);
        world
            .resource_mut::<ActorEditor>()
            .draft
            .as_mut()
            .unwrap()
            .set_text(&FieldKey::Speed, "95".into());
        world
            .resource_mut::<ActorEditor>()
            .requests
            .push(Action::Revert);
        apply_requests(&mut world);
        let editor = world.resource::<ActorEditor>();
        assert_eq!(editor.selected, saved_path);
        assert_eq!(
            editor.draft.as_ref().unwrap().text(&FieldKey::Speed),
            saved_speed
        );
        assert!(!editor.dirty());
        let rover = editor
            .entries
            .iter()
            .position(|e| e.path.ends_with("rover.json"))
            .unwrap();
        world
            .resource_mut::<ActorEditor>()
            .requests
            .extend([Action::Select(rover), Action::Duplicate]);
        apply_requests(&mut world);
        let copy = world.resource::<ActorEditor>().draft.clone();
        assert_eq!(copy.as_ref().unwrap().model, "rover");
        world
            .resource_mut::<ActorEditor>()
            .draft
            .as_mut()
            .unwrap()
            .set_text(&FieldKey::Speed, "99".into());
        world
            .resource_mut::<ActorEditor>()
            .requests
            .push(Action::Revert);
        apply_requests(&mut world);
        assert_eq!(world.resource::<ActorEditor>().draft, copy);
        assert!(world.resource::<ActorEditor>().dirty());
        world
            .resource_mut::<ActorEditor>()
            .requests
            .push(Action::New);
        apply_requests(&mut world);
        world
            .resource_mut::<ActorEditor>()
            .requests
            .push(Action::DiscardPending);
        apply_requests(&mut world);
        let new_draft = world.resource::<ActorEditor>().draft.clone();
        world
            .resource_mut::<ActorEditor>()
            .draft
            .as_mut()
            .unwrap()
            .set_text(&FieldKey::Name, "Changed new draft".into());
        world
            .resource_mut::<ActorEditor>()
            .requests
            .push(Action::Revert);
        apply_requests(&mut world);
        assert_eq!(world.resource::<ActorEditor>().draft, new_draft);
        assert!(world.resource::<ActorEditor>().dirty());
    }

    #[test]
    fn switching_presets_requires_discarding_unsaved_fields() {
        let temp = Temp::new("unsaved");
        let mut world = editor_world(&temp);
        world
            .resource_mut::<ActorEditor>()
            .draft
            .as_mut()
            .unwrap()
            .set_text(&FieldKey::Name, "Keep this draft".into());
        let original = world.resource::<ActorEditor>().selected.clone();
        world
            .resource_mut::<ActorEditor>()
            .requests
            .push(Action::Select(1));
        apply_requests(&mut world);
        let editor = world.resource::<ActorEditor>();
        assert_eq!(editor.selected, original);
        assert!(editor.pending.is_some());
        assert_eq!(
            editor.draft.as_ref().unwrap().text(&FieldKey::Name),
            "Keep this draft"
        );
        world
            .resource_mut::<ActorEditor>()
            .requests
            .push(Action::DiscardPending);
        apply_requests(&mut world);
        assert_ne!(world.resource::<ActorEditor>().selected, original);
    }

    #[test]
    fn closing_a_dirty_editor_requires_discard_and_cancel_preserves_it() {
        use bevy::ecs::system::RunSystemOnce;
        let temp = Temp::new("close");
        let mut world = editor_world(&temp);
        world.init_resource::<Messages<AppExit>>();
        world.init_resource::<Messages<bevy::window::WindowCloseRequested>>();
        let window = world
            .spawn((Window::default(), bevy::window::PrimaryWindow))
            .id();
        world
            .resource_mut::<ActorEditor>()
            .draft
            .as_mut()
            .unwrap()
            .set_text(&FieldKey::Name, "Unsaved actor".into());
        world.write_message(bevy::window::WindowCloseRequested { window });
        world.run_system_once(close_requested).unwrap();
        apply_requests(&mut world);
        assert!(world.resource::<Messages<AppExit>>().is_empty());
        assert!(world.resource::<ActorEditor>().pending.is_some());
        world
            .resource_mut::<ActorEditor>()
            .requests
            .push(Action::KeepDraft);
        apply_requests(&mut world);
        assert_eq!(
            world
                .resource::<ActorEditor>()
                .draft
                .as_ref()
                .unwrap()
                .text(&FieldKey::Name),
            "Unsaved actor"
        );
        assert!(world.resource::<ActorEditor>().pending.is_none());
        world
            .resource_mut::<ActorEditor>()
            .requests
            .push(Action::Close);
        apply_requests(&mut world);
        world
            .resource_mut::<ActorEditor>()
            .requests
            .push(Action::DiscardPending);
        apply_requests(&mut world);
        assert_eq!(world.resource::<Messages<AppExit>>().len(), 1);
    }

    #[test]
    fn editor_ui_runs_without_a_simulation_driver() {
        use bevy::gltf::Gltf;
        use bevy::world_serialization::WorldAsset;
        let temp = Temp::new("ui");
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin::default(),
            bevy::state::app::StatesPlugin,
            bevy::input::InputPlugin,
            bevy::gizmos::GizmoPlugin,
        ))
        .init_asset::<Mesh>()
        .init_asset::<StandardMaterial>()
        .init_asset::<Image>()
        .init_asset::<Gltf>()
        .init_asset::<WorldAsset>()
        .init_asset::<AnimationClip>()
        .init_asset::<AnimationGraph>()
        .init_asset::<Font>()
        .init_asset::<bevy::mesh::skinning::SkinnedMeshInverseBindposes>()
        .init_resource::<InputFocus>();
        configure(&mut app, temp.0.clone());
        app.update();
        app.update();
        let mut saves = app.world_mut().query_filtered::<Entity, With<SaveButton>>();
        assert_eq!(saves.iter(app.world()).count(), 1);
        assert!(
            !app.world()
                .contains_resource::<npc_framework::prelude::SimClock>()
        );
        assert!(app.world().resource::<ActorEditor>().errors.is_empty());
        // Typing changes the actual save target without rebuilding the form
        // (which would replace the focused EditableText and reset its caret).
        let mut field_query = app.world_mut().query::<(Entity, &EditorField)>();
        let name_field = field_query
            .iter(app.world())
            .find(|(_, field)| field.0 == FieldKey::Name)
            .unwrap()
            .0;
        let mut destination_query = app
            .world_mut()
            .query_filtered::<Entity, With<DestinationText>>();
        let destination = destination_query.single(app.world()).unwrap();
        app.world_mut()
            .get_mut::<EditableText>(name_field)
            .unwrap()
            .editor_mut()
            .set_text("Renamed scout");
        app.update();
        assert!(
            app.world().get::<EditorField>(name_field).is_some(),
            "typing must retain the same editable field"
        );
        assert_eq!(
            app.world()
                .get::<EditableText>(name_field)
                .unwrap()
                .value()
                .to_string(),
            "Renamed scout"
        );
        assert_eq!(
            app.world().get::<Text>(destination).unwrap().0,
            temp.0.join("renamed-scout.json").display().to_string()
        );
        let original_name = app
            .world()
            .resource::<ActorEditor>()
            .original
            .as_ref()
            .unwrap()
            .text(&FieldKey::Name);
        app.world_mut()
            .get_mut::<EditableText>(name_field)
            .unwrap()
            .editor_mut()
            .set_text(&original_name);
        app.update();
        assert_eq!(
            app.world().get::<Text>(destination).unwrap().0,
            temp.0.join("human.json").display().to_string(),
            "restoring the original name restores its stable file ID"
        );
    }

    #[test]
    fn native_field_layout_reserves_text_content_at_normal_and_hidpi_scale() {
        // This is the installed Bevy UI layout fixture, without a renderer:
        // calculate real ComputedNode content rectangles, not style constants.
        use bevy::app::{HierarchyPropagatePlugin, PropagateSet};
        use bevy::camera::{ComputedCameraValues, RenderTargetInfo, Viewport};
        use bevy::text::{FontCx, LineHeight, RemSize, ScaleCx, TextPipeline};
        use bevy::ui::{
            ComputedUiRenderTargetInfo, ComputedUiTargetCamera, UiScale, sync_font_size_to_em_size,
            ui_layout_system,
            ui_surface::UiSurface,
            update::propagate_ui_target_cameras,
            widget::{EditableTextContentSizeState, update_editable_text_content_size},
        };
        for scale in [1.0, 2.0] {
            let physical = UVec2::new((840.0 * scale) as u32, (1360.0 * scale) as u32);
            let mut app = App::new();
            app.add_plugins((
                MinimalPlugins,
                AssetPlugin::default(),
                HierarchyPropagatePlugin::<ComputedUiTargetCamera>::new(PostUpdate),
                HierarchyPropagatePlugin::<ComputedUiRenderTargetInfo>::new(PostUpdate),
            ))
            .init_asset::<Font>()
            .init_resource::<FontCx>()
            .init_resource::<RemSize>()
            .init_resource::<ScaleCx>()
            .init_resource::<TextPipeline>()
            .init_resource::<UiScale>()
            .init_resource::<UiSurface>()
            .register_required_components::<EditableText, EditableTextContentSizeState>()
            .configure_sets(
                PostUpdate,
                PropagateSet::<ComputedUiTargetCamera>::default()
                    .after(propagate_ui_target_cameras)
                    .before(update_editable_text_content_size),
            )
            .configure_sets(
                PostUpdate,
                PropagateSet::<ComputedUiRenderTargetInfo>::default()
                    .after(propagate_ui_target_cameras)
                    .before(update_editable_text_content_size),
            )
            .add_systems(
                PostUpdate,
                (
                    propagate_ui_target_cameras,
                    sync_font_size_to_em_size,
                    update_editable_text_content_size,
                    ui_layout_system,
                )
                    .chain(),
            );
            app.world_mut().spawn((
                Camera2d,
                Camera {
                    computed: ComputedCameraValues {
                        target_info: Some(RenderTargetInfo {
                            physical_size: physical,
                            scale_factor: scale,
                        }),
                        ..default()
                    },
                    viewport: Some(Viewport {
                        physical_size: physical,
                        ..default()
                    }),
                    ..default()
                },
                IsDefaultUiCamera,
            ));
            let mut draft = Draft::from_preset(&actors::builtin_preset("rover").unwrap());
            draft.set_text(&FieldKey::Detection, "per_roll".into());
            app.world_mut()
                .commands()
                .spawn(Node {
                    width: px(420),
                    height: px(680),
                    flex_direction: FlexDirection::Column,
                    ..default()
                })
                .with_children(|p| {
                    for (i, key) in [
                        FieldKey::Name,
                        FieldKey::Speed,
                        FieldKey::Carry,
                        FieldKey::Detection,
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        text_field(p, &draft, key, "Field", i as i32, false);
                    }
                    text_field(p, &draft, FieldKey::Description, "Description", 5, true);
                });
            app.world_mut().flush();
            app.update();
            app.update();
            let mut fields = app.world_mut().query::<(
                &EditorField,
                &ComputedNode,
                &EditableText,
                &TextFont,
                &LineHeight,
            )>();
            let fields: Vec<_> = fields.iter(app.world()).collect();
            assert_eq!(fields.len(), 5);
            for (key, node, text, font, line_height) in fields {
                let font_size = font
                    .font_size
                    .eval(physical.as_vec2() / scale, RemSize::default());
                let line_height = match *line_height {
                    LineHeight::Px(px) => px,
                    LineHeight::RelativeToFont(multiplier) => multiplier * font_size,
                };
                let lines = if key.0 == FieldKey::Description {
                    3.0
                } else {
                    1.0
                };
                let content_height = node.content_box().height() * node.inverse_scale_factor;
                assert!(
                    content_height + 0.5 >= lines * line_height,
                    "{:?}: {content_height}px of text content at scale {scale}, expected {lines} lines of {line_height}px",
                    key.0
                );
                assert_eq!(text.value().to_string(), draft.text(&key.0));
            }
        }
    }
}
