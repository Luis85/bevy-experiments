//! The perf panel (F2): profiler timings per feature, phase and system (p50/p95/max with a
//! sparkline per row), FPS and frame-time graphs, the entity count and render stats (spec §15
//! D). Observe-only.
//!
//! The profiler lives in `fl_observe`, which the client does not link (spec §5.1); the game
//! inserts a [`ProfilerSource`] that returns its snapshot as JSON.
use std::collections::{BTreeMap, VecDeque};
use std::time::Duration;

use bevy::diagnostic::{
    DiagnosticPath, DiagnosticsStore, EntityCountDiagnosticsPlugin, FrameTimeDiagnosticsPlugin,
};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use serde::Deserialize;

use crate::view::UnitView;

/// Returns the profiler snapshot (`fl_observe::PerfSnapshot`) as JSON; inserted by the game.
#[derive(Resource, Clone, Copy)]
pub struct ProfilerSource(pub fn() -> serde_json::Value);

/// How often the open panel reads the profiler.
const SAMPLE_PERIOD: Duration = Duration::from_millis(250);
/// Samples kept per feature and phase row for its sparkline.
const ROW_HISTORY: usize = 120;
/// Systems listed, slowest p95 first.
const SYSTEM_ROWS: usize = 40;

/// Rolling statistics of one series, in milliseconds (`fl_observe::Stat`).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct PerfStat {
    pub mean_ms: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub max_ms: f64,
    pub samples: u64,
}

/// One system's timings (`fl_observe::SystemStat`).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct PerfSystem {
    pub name: String,
    pub feature: String,
    pub stat: PerfStat,
    pub history_ms: Vec<f32>,
}

/// The profiler snapshot as the panel reads it (`fl_observe::PerfSnapshot`).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct PerfTables {
    pub ticks: u64,
    pub tick: PerfStat,
    pub phases: Vec<(String, PerfStat)>,
    pub features: Vec<(String, PerfStat)>,
    pub systems: Vec<PerfSystem>,
    pub history_ms: Vec<f32>,
}

/// The perf panel: whether it is open, the last profiler sample and the p50 history of every
/// feature and phase row.
#[derive(Resource, Default)]
pub struct PerfPanel {
    pub open: bool,
    pub tables: PerfTables,
    /// p50 per sample, oldest first, by `"feature:<name>"` or `"phase:<name>"`.
    pub history: BTreeMap<String, VecDeque<f32>>,
    since_sample: Option<Duration>,
}

impl PerfPanel {
    /// Takes a new profiler sample and appends each feature's and phase's p50 to its history.
    pub fn record(&mut self, tables: PerfTables) {
        let rows = tables
            .features
            .iter()
            .map(|(name, stat)| (format!("feature:{name}"), stat))
            .chain(
                tables
                    .phases
                    .iter()
                    .map(|(name, stat)| (format!("phase:{name}"), stat)),
            );
        for (key, stat) in rows {
            let history = self.history.entry(key).or_default();
            if history.len() == ROW_HISTORY {
                history.pop_front();
            }
            history.push_back(stat.p50_ms as f32);
        }
        self.tables = tables;
    }
}

/// Opens or closes the perf panel on F2.
pub(crate) fn toggle_perf_panel(keys: Res<ButtonInput<KeyCode>>, mut panel: ResMut<PerfPanel>) {
    if keys.just_pressed(KeyCode::F2) {
        panel.open = !panel.open;
    }
}

/// Reads the profiler every [`SAMPLE_PERIOD`] of real time while the panel is open.
pub(crate) fn sample_perf(
    time: Res<Time<Real>>,
    source: Option<Res<ProfilerSource>>,
    mut panel: ResMut<PerfPanel>,
) {
    let Some(source) = source else {
        return;
    };
    let elapsed = panel
        .since_sample
        .map_or(SAMPLE_PERIOD, |t| t + time.delta());
    if elapsed < SAMPLE_PERIOD {
        panel.since_sample = Some(elapsed);
        return;
    }
    panel.since_sample = Some(Duration::ZERO);
    match serde_json::from_value::<PerfTables>((source.0)()) {
        Ok(tables) => panel.record(tables),
        Err(err) => tracing::warn!(%err, "the profiler snapshot does not decode"),
    }
}

/// The history of a diagnostic, oldest first, as `f32`.
fn diagnostic_values(store: &DiagnosticsStore, path: &DiagnosticPath) -> Vec<f32> {
    store
        .get(path)
        .map(|d| d.values().map(|&v| v as f32).collect())
        .unwrap_or_default()
}

fn smoothed(store: Option<&DiagnosticsStore>, path: &DiagnosticPath) -> Option<f64> {
    store.and_then(|s| s.get(path)).and_then(|d| d.smoothed())
}

/// Client frame stats: smoothed FPS and frame time, entity count, unit visuals (total and
/// visible) and visible meshes. `null` where a diagnostic is not installed.
pub fn frame_stats(world: &mut World) -> serde_json::Value {
    let store = world.get_resource::<DiagnosticsStore>();
    let fps = smoothed(store, &FrameTimeDiagnosticsPlugin::FPS);
    let frame_time_ms = smoothed(store, &FrameTimeDiagnosticsPlugin::FRAME_TIME);
    let entities = world.entities().count_spawned();
    let units = world
        .query_filtered::<&ViewVisibility, With<UnitView>>()
        .iter(world)
        .fold((0_u32, 0_u32), |(all, shown), v| {
            (all + 1, shown + u32::from(v.get()))
        });
    let visible_meshes = world
        .query_filtered::<&ViewVisibility, With<Mesh3d>>()
        .iter(world)
        .filter(|v| v.get())
        .count();
    serde_json::json!({
        "fps": fps,
        "frame_time_ms": frame_time_ms,
        "entities": entities,
        "unit_visuals": units.0,
        "visible_unit_visuals": units.1,
        "visible_meshes": visible_meshes,
    })
}

/// Draws `values` as a line scaled to their maximum.
fn sparkline(ui: &mut egui::Ui, values: impl ExactSizeIterator<Item = f32>, size: egui::Vec2) {
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let values: Vec<f32> = values.collect();
    if values.len() < 2 {
        return;
    }
    let max = values.iter().copied().fold(f32::EPSILON, f32::max);
    let last = (values.len() - 1) as f32;
    let points = values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            egui::pos2(
                rect.left() + rect.width() * i as f32 / last,
                rect.bottom() - rect.height() * (v / max).clamp(0.0, 1.0),
            )
        })
        .collect();
    let stroke = egui::Stroke::new(1.0, ui.visuals().text_color());
    ui.painter().add(egui::Shape::line(points, stroke));
}

const SPARK: egui::Vec2 = egui::vec2(120.0, 16.0);
const GRAPH: egui::Vec2 = egui::vec2(260.0, 40.0);

/// One table row: name, p50, p95, max and a sparkline.
fn stat_row(
    ui: &mut egui::Ui,
    name: &str,
    stat: &PerfStat,
    history: impl ExactSizeIterator<Item = f32>,
) {
    ui.monospace(name);
    ui.monospace(format!("{:.3}", stat.p50_ms));
    ui.monospace(format!("{:.3}", stat.p95_ms));
    ui.monospace(format!("{:.3}", stat.max_ms));
    sparkline(ui, history, SPARK);
    ui.end_row();
}

fn table_header(ui: &mut egui::Ui, first: &str) {
    for title in [first, "p50 ms", "p95 ms", "max ms", ""] {
        ui.strong(title);
    }
    ui.end_row();
}

/// The render stats the panel shows.
#[derive(Default)]
struct RenderStats {
    unit_visuals: usize,
    visible_unit_visuals: usize,
    visible_meshes: usize,
}

/// Draws the perf panel.
pub(crate) fn perf_window(
    mut contexts: EguiContexts,
    mut panel: ResMut<PerfPanel>,
    diagnostics: Option<Res<DiagnosticsStore>>,
    units: Query<&ViewVisibility, With<UnitView>>,
    meshes: Query<&ViewVisibility, With<Mesh3d>>,
    source: Option<Res<ProfilerSource>>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    let store = diagnostics.as_deref();
    let render = RenderStats {
        unit_visuals: units.iter().len(),
        visible_unit_visuals: units.iter().filter(|v| v.get()).count(),
        visible_meshes: meshes.iter().filter(|v| v.get()).count(),
    };
    let mut open = panel.open;
    egui::Window::new("Perf")
        .open(&mut open)
        .default_width(560.0)
        .show(ctx, |ui| {
            client_section(ui, store, &render);
            ui.separator();
            if source.is_none() {
                ui.label("no profiler in this build");
                return;
            }
            sim_section(ui, &panel);
        });
    panel.open = open;
    Ok(())
}

fn client_section(ui: &mut egui::Ui, store: Option<&DiagnosticsStore>, render: &RenderStats) {
    let fps = smoothed(store, &FrameTimeDiagnosticsPlugin::FPS);
    let frame = smoothed(store, &FrameTimeDiagnosticsPlugin::FRAME_TIME);
    let entities = smoothed(store, &EntityCountDiagnosticsPlugin::ENTITY_COUNT);
    let show = |v: Option<f64>, decimals: usize| {
        v.map_or_else(|| "-".to_owned(), |v| format!("{v:.decimals$}"))
    };
    ui.monospace(format!(
        "fps {}   frame {} ms   entities {}",
        show(fps, 0),
        show(frame, 2),
        show(entities, 0)
    ));
    if let Some(store) = store {
        ui.horizontal(|ui| {
            ui.label("fps");
            let values = diagnostic_values(store, &FrameTimeDiagnosticsPlugin::FPS);
            sparkline(ui, values.into_iter(), GRAPH);
            ui.label("frame ms");
            let values = diagnostic_values(store, &FrameTimeDiagnosticsPlugin::FRAME_TIME);
            sparkline(ui, values.into_iter(), GRAPH);
        });
    }
    ui.monospace(format!(
        "unit visuals {} (visible {})   visible meshes {}",
        render.unit_visuals, render.visible_unit_visuals, render.visible_meshes
    ));
}

fn sim_section(ui: &mut egui::Ui, panel: &PerfPanel) {
    let tables = &panel.tables;
    let history = |key: String| {
        panel
            .history
            .get(&key)
            .map(|h| h.iter().copied().collect::<Vec<_>>())
            .unwrap_or_default()
    };
    egui::ScrollArea::vertical()
        .max_height(480.0)
        .show(ui, |ui| {
            ui.monospace(format!("sim ticks {}", tables.ticks));
            egui::Grid::new("perf_ticks_phases")
                .striped(true)
                .show(ui, |ui| {
                    table_header(ui, "phase");
                    stat_row(ui, "tick", &tables.tick, tables.history_ms.iter().copied());
                    for (name, stat) in &tables.phases {
                        let values = history(format!("phase:{name}"));
                        stat_row(ui, name, stat, values.into_iter());
                    }
                });
            ui.separator();
            egui::Grid::new("perf_features")
                .striped(true)
                .show(ui, |ui| {
                    table_header(ui, "feature");
                    for (name, stat) in &tables.features {
                        let values = history(format!("feature:{name}"));
                        stat_row(ui, name, stat, values.into_iter());
                    }
                });
            ui.separator();
            let mut systems: Vec<&PerfSystem> = tables.systems.iter().collect();
            systems.sort_by(|a, b| b.stat.p95_ms.total_cmp(&a.stat.p95_ms));
            egui::Grid::new("perf_systems")
                .striped(true)
                .show(ui, |ui| {
                    table_header(ui, "system (slowest p95)");
                    for system in systems.into_iter().take(SYSTEM_ROWS) {
                        let values = system.history_ms.iter().copied();
                        stat_row(ui, &system.name, &system.stat, values);
                    }
                });
        });
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn profiler_json_decodes_and_rows_keep_their_history() {
        let stat = |p50: f64| json!({"mean_ms": p50, "p50_ms": p50, "p95_ms": 2.0, "max_ms": 3.0, "samples": 3});
        let snapshot = |p50: f64| {
            json!({
                "ticks": 3,
                "tick": stat(1.0),
                "phases": [["Act", stat(p50)]],
                "features": [["core.movement", stat(p50)]],
                "systems": [{
                    "name": "fl_sim::movement::move_system",
                    "feature": "core.movement",
                    "stat": stat(0.2),
                    "history_ms": [0.1, 0.2, 0.3],
                }],
                "history_ms": [1.0, 1.5, 3.0],
            })
        };
        let mut panel = PerfPanel::default();
        for p50 in [0.25, 0.5] {
            let tables: PerfTables = serde_json::from_value(snapshot(p50)).expect("decodes");
            panel.record(tables);
        }
        assert_eq!(panel.tables.ticks, 3);
        assert_eq!(panel.tables.systems[0].history_ms, [0.1, 0.2, 0.3]);
        let history: Vec<f32> = panel.history["feature:core.movement"]
            .iter()
            .copied()
            .collect();
        assert_eq!(history, [0.25, 0.5]);
        assert_eq!(panel.history["phase:Act"].len(), 2);
    }

    #[test]
    fn frame_stats_count_unit_visuals() {
        let mut world = World::new();
        world.spawn((
            UnitView {
                sim_id: fl_protocol::SimId(1),
                unit: fl_protocol::UnitTypeId(0),
                owner: fl_protocol::PlayerId(0),
            },
            ViewVisibility::default(),
        ));
        let stats = frame_stats(&mut world);
        assert_eq!(stats["unit_visuals"], 1);
        assert_eq!(stats["visible_unit_visuals"], 0);
        assert_eq!(stats["fps"], serde_json::Value::Null);
    }
}
