//! Perf panel (feature `perf`; F4 in the village app): frame time, sim tick time
//! and the top [`TOP`] mechanics and systems by total time with their rolling
//! p50/p95/max from the `npc_perf` profiler. Mechanics over their p95 budget
//! (and systems of such mechanics) are red. The report is cached and
//! recomputed at most every [`REFRESH_SECS`] while the panel is visible,
//! with `PerfCollector::report_limited` (percentiles only for the rows shown,
//! so the cost does not grow with the number of systems).

use std::fmt::Write as _;

use bevy::prelude::*;
use npc_perf::{PerfHandle, PerfReport, clip};

use npc_kernel::SimControl;

use crate::timing::SimTickTiming;
use crate::{DebugPanels, ViewSystems};

/// Rows per table.
pub const TOP: usize = 10;
/// Seconds between refreshes (report recomputed at most this often).
pub const REFRESH_SECS: f32 = 0.5;
/// Text spans: title, mechanic header, TOP rows, system header, TOP rows, help.
const SPANS: usize = 4 + 2 * TOP;

const NORMAL: Color = Color::srgb(0.92, 0.95, 0.9);
const HEADER: Color = Color::srgb(0.6, 0.85, 1.0);
const OVER: Color = Color::srgb(1.0, 0.3, 0.25);

#[derive(Component)]
struct PerfPanel;

/// The `i`-th text span of the panel.
#[derive(Component)]
struct PerfRow(usize);

/// Smoothed real frame time and the cached report.
#[derive(Default)]
struct FrameMs {
    ema: Option<f32>,
    since_refresh: f32,
    report: Option<PerfReport>,
}

pub(crate) fn plugin(app: &mut App) {
    app.add_systems(Startup, spawn_panel)
        .add_systems(Update, update_panel.in_set(ViewSystems::Present));
}

fn spawn_panel(mut commands: Commands) {
    let font = TextFont {
        font_size: FontSize::Px(12.0),
        ..default()
    };
    commands
        .spawn((
            Name::new("perf panel"),
            PerfPanel,
            Text::new(""),
            font.clone(),
            TextColor(NORMAL),
            BackgroundColor(Color::srgba(0.05, 0.06, 0.08, 0.78)),
            Node {
                position_type: PositionType::Absolute,
                bottom: px(10),
                left: px(10),
                padding: UiRect::all(px(8)),
                ..default()
            },
            Visibility::Hidden,
        ))
        .with_children(|panel| {
            for i in 0..SPANS {
                panel.spawn((
                    TextSpan::new(""),
                    font.clone(),
                    TextColor(NORMAL),
                    PerfRow(i),
                ));
            }
        });
}

/// The panel's lines as `(text, color)`, exactly [`SPANS`] of them.
fn lines(
    report: Option<&PerfReport>,
    frame_ms: Option<f32>,
    timing: Option<&SimTickTiming>,
    speed: Option<&SimControl>,
) -> Vec<(String, Color)> {
    let mut out = Vec::with_capacity(SPANS);
    let mut title = String::from("[F4] perf");
    match frame_ms {
        Some(ms) if ms > 0.0 => {
            let _ = write!(title, "   frame {ms:.2} ms ({:.0} fps)", 1000.0 / ms);
        }
        _ => title.push_str("   frame -"),
    }
    match timing.and_then(|t| t.ema_us) {
        Some(us) => {
            let _ = write!(title, "   tick {us:.1} us");
        }
        None => title.push_str("   tick -"),
    }
    if let Some(sp) = speed {
        let _ = write!(
            title,
            " (x{}{})",
            sp.multiplier,
            if sp.paused { ", paused" } else { "" }
        );
    }
    if let Some(r) = report {
        let _ = write!(title, "   frames {}", r.frames);
    }
    out.push((title + "\n", NORMAL));

    let Some(report) = report else {
        out.push(("profiler not installed (no PerfHandle)\n".into(), OVER));
        out.resize(SPANS, (String::new(), NORMAL));
        return out;
    };
    out.push((
        format!(
            "{:<22} {:>8} {:>8} {:>8} {:>7}\n",
            "MECHANIC (us)", "p50", "p95", "max", "budget"
        ),
        HEADER,
    ));
    for i in 0..TOP {
        out.push(match report.mechanics.get(i) {
            Some(m) => (
                format!(
                    "{:<22} {:>8.1} {:>8.1} {:>8.1} {:>7}\n",
                    clip(&m.mechanic, 22),
                    m.p50_us,
                    m.p95_us,
                    m.max_us,
                    m.budget_us
                        .map_or_else(|| "-".to_owned(), |b| b.to_string()),
                ),
                if m.over_budget { OVER } else { NORMAL },
            ),
            None => (String::new(), NORMAL),
        });
    }
    out.push((
        format!(
            "{:<44} {:<13} {:>7} {:>7} {:>8}\n",
            "SYSTEM (us)", "mechanic", "p50", "p95", "max"
        ),
        HEADER,
    ));
    for i in 0..TOP {
        out.push(match report.systems.get(i) {
            Some(s) => {
                let over = report.mechanic(&s.mechanic).is_some_and(|m| m.over_budget);
                (
                    format!(
                        "{:<44} {:<13} {:>7.1} {:>7.1} {:>8.1}\n",
                        clip(&s.name, 44),
                        clip(&s.mechanic, 13),
                        s.p50_us,
                        s.p95_us,
                        s.max_us
                    ),
                    if over { OVER } else { NORMAL },
                )
            }
            None => (String::new(), NORMAL),
        });
    }
    out.push(("[click] select a unit".into(), HEADER));
    out.resize(SPANS, (String::new(), NORMAL));
    out
}

#[allow(clippy::too_many_arguments)]
fn update_panel(
    time: Res<Time<Real>>,
    panels: Res<DebugPanels>,
    perf: Option<Res<PerfHandle>>,
    timing: Option<Res<SimTickTiming>>,
    speed: Option<Res<SimControl>>,
    mut frame: Local<FrameMs>,
    mut panel: Query<&mut Visibility, With<PerfPanel>>,
    mut rows: Query<(&PerfRow, &mut TextSpan, &mut TextColor)>,
) {
    let dt_ms = time.delta_secs() * 1000.0;
    if dt_ms > 0.0 {
        frame.ema = Some(frame.ema.map_or(dt_ms, |e| e + (dt_ms - e) * 0.1));
    }
    let want = if panels.perf {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    let mut became_visible = false;
    for mut v in &mut panel {
        if *v != want {
            became_visible = panels.perf;
            *v = want;
        }
    }
    if !panels.perf {
        return;
    }
    frame.since_refresh += time.delta_secs();
    if frame.since_refresh < REFRESH_SECS && !became_visible {
        return;
    }
    frame.since_refresh = 0.0;
    frame.report = perf.map(|p| p.0.report_limited(TOP, usize::MAX));
    let lines = lines(
        frame.report.as_ref(),
        frame.ema,
        timing.as_deref(),
        speed.as_deref(),
    );
    for (row, mut span, mut color) in &mut rows {
        let Some((text, c)) = lines.get(row.0) else {
            continue;
        };
        if span.0 != *text {
            span.0.clone_from(text);
        }
        if color.0 != *c {
            color.0 = *c;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use npc_perf::PerfCollector;

    #[test]
    fn lines_have_fixed_count_and_red_over_budget() {
        assert_eq!(lines(None, None, None, None).len(), SPANS);
        let collector = PerfCollector::new(16);
        collector.set_min_samples(1);
        let mut budgets = npc_perf::PerfBudgets::default();
        budgets.set("needs", 1);
        collector.set_budgets(budgets);
        collector.record_for_test("npc_needs::decay_needs_system", 50);
        collector.record_for_test("npc_kernel::journal::flush_journal_system", 5);
        collector.end_frame();
        // What `update_panel` caches: every mechanic, the top systems.
        let report = collector.report_limited(TOP, usize::MAX);
        assert_eq!(report, collector.report());
        const { assert!(REFRESH_SECS >= 0.5) };
        let out = lines(
            Some(&report),
            Some(16.6),
            None,
            Some(&SimControl::default()),
        );
        assert_eq!(out.len(), SPANS);
        assert!(out[0].0.contains("60 fps"), "{}", out[0].0);
        let needs = out
            .iter()
            .find(|(t, _)| t.starts_with("needs"))
            .expect("needs row");
        assert_eq!(needs.1, OVER);
        let journal = out
            .iter()
            .find(|(t, _)| t.starts_with("journal"))
            .expect("journal row");
        assert_eq!(journal.1, NORMAL);
        let system = out
            .iter()
            .find(|(t, _)| t.contains("decay_needs_system"))
            .expect("system row");
        assert_eq!(system.1, OVER);
    }
}
