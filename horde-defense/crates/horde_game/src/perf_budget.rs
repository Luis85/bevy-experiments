//! Presentation performance budget. `docs/PERF_BUDGET.json` is the single gate
//! source the measurement script reads; these constants mirror it, and the
//! tests below hold the two equal in both directions, so neither side can
//! change alone. Nothing here touches the simulation.
#![cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "the budget is read by the tests and docs/PERF_BUDGET.json now; the batched battlefield and corpse cache consume it later"
    )
)]
use crate::{animation, ui};

/// -VsyncOff CPU frame (main + render world) p99 gate for the progressed-peak
/// and stress scenes, median of 3 runs. 12.0 ms is only the pipelined-rendering
/// decision threshold, never an acceptance gate.
pub const CPU_FRAME_P99_MS_MAX: f64 = 10.0;
/// Frames over 50 ms allowed in any gated scene, worst repeat.
pub const FRAMES_OVER_50MS_MAX: usize = 0;
/// Scenes that carry the CPU p99 gate.
pub const CPU_GATED_SCENES: &[&str] = &["progressed-peak", "stress"];

/// Most enemies a production save can field: the authored cap plus the boss.
pub const ENEMIES_CEILING: usize = horde_core::progression::GAMEPLAY_ENEMY_CAP + 1;
/// Enemies drawn by the review-only stress scene.
pub const ENEMIES_STRESS: usize = 400;
/// Newest settled corpses drawn from the cached mesh.
pub const CORPSES_SETTLED_DRAWN: usize = 2048;
/// Collapsing corpses sampled per frame.
pub const CORPSES_COLLAPSING_DRAWN: usize = 64;
/// Longest any battlefield effect stays on screen.
pub const FX_LIFETIME_MAX_SECONDS: f64 = 2.0;

/// Hard egui ceilings for the battlefield painter, tightened to measured x 1.25
/// once the batched meshes land, never above these.
pub const BATTLEFIELD_PRIMITIVES_MAX: usize = 48;
/// Most extra primitives 400 enemies may cost over 52.
pub const PRIMITIVE_GROWTH_52_TO_400_MAX: usize = 2;
/// Battlefield vertex ceilings per measured scene.
pub const BATTLEFIELD_VERTICES_MAX: &[(&str, usize)] =
    &[("progressed-peak", 40_000), ("stress", 72_000)];

/// Whole-number presentation caps, keyed as in `PERF_BUDGET.json` `caps`
/// (whose one float, `fx_lifetime_max_seconds`, is `FX_LIFETIME_MAX_SECONDS`).
pub const CAPS: &[(&str, usize)] = &[
    ("enemies_ceiling", ENEMIES_CEILING),
    ("enemies_stress", ENEMIES_STRESS),
    ("corpses_settled_drawn", CORPSES_SETTLED_DRAWN),
    ("corpses_collapsing_drawn", CORPSES_COLLAPSING_DRAWN),
    ("particles", animation::MAX_PARTICLES),
    ("particle_events", animation::EVENT_BUDGET),
    ("scenes", animation::MAX_SCENES),
    ("damage_popups", ui::damage::LIMIT),
    ("loot_flyups", ui::loot::LIMIT),
];

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::collections::BTreeSet;

    const BUDGET: &str = include_str!("../../../docs/PERF_BUDGET.json");
    const ACCEPTANCE: &str = include_str!("../../../docs/ACCEPTANCE.md");

    fn budget() -> Value {
        serde_json::from_str(BUDGET).expect("docs/PERF_BUDGET.json parses")
    }

    fn object<'a>(value: &'a Value, path: &str) -> &'a serde_json::Map<String, Value> {
        value
            .pointer(path)
            .and_then(Value::as_object)
            .unwrap_or_else(|| panic!("PERF_BUDGET.json {path} is not an object"))
    }

    fn number(value: &Value, path: &str) -> f64 {
        value
            .pointer(path)
            .and_then(Value::as_f64)
            .unwrap_or_else(|| panic!("PERF_BUDGET.json {path} is not a number"))
    }

    fn whole(value: &Value, path: &str) -> usize {
        value
            .pointer(path)
            .and_then(Value::as_u64)
            .and_then(|whole| usize::try_from(whole).ok())
            .unwrap_or_else(|| panic!("PERF_BUDGET.json {path} is not a whole number"))
    }

    fn keys(map: &serde_json::Map<String, Value>) -> BTreeSet<&str> {
        map.keys().map(String::as_str).collect()
    }

    /// Both directions of a `(key, value)` table against a JSON object of whole
    /// numbers: every Rust row is in the file with the same value, and every
    /// file key is a Rust row.
    fn assert_table_matches(
        table: &[(&str, usize)],
        json: &serde_json::Map<String, Value>,
        path: &str,
    ) {
        let rust: BTreeSet<&str> = table.iter().map(|(key, _)| *key).collect();
        assert_eq!(rust.len(), table.len(), "{path}: duplicate Rust keys");
        assert_eq!(
            keys(json),
            rust,
            "{path}: the JSON and Rust key sets differ"
        );
        for (key, value) in table {
            assert_eq!(
                json[*key]
                    .as_u64()
                    .and_then(|whole| usize::try_from(whole).ok()),
                Some(*value),
                "{path}/{key}: JSON and Rust disagree"
            );
        }
    }

    #[test]
    fn perf_budget_json_matches_presentation_caps() {
        let json = budget();

        // caps: every whole-number cap is a CAPS row, plus the one float.
        let mut caps = object(&json, "/caps").clone();
        caps.remove("fx_lifetime_max_seconds");
        assert_table_matches(CAPS, &caps, "/caps");
        assert_eq!(
            number(&json, "/caps/fx_lifetime_max_seconds"),
            FX_LIFETIME_MAX_SECONDS
        );
        let cap = |name: &str| {
            CAPS.iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| *value)
        };
        assert_eq!(cap("particles"), Some(crate::animation::MAX_PARTICLES));
        assert_eq!(cap("particle_events"), Some(crate::animation::EVENT_BUDGET));
        assert_eq!(cap("scenes"), Some(crate::animation::MAX_SCENES));
        assert_eq!(cap("damage_popups"), Some(crate::ui::damage::LIMIT));
        assert_eq!(cap("loot_flyups"), Some(crate::ui::loot::LIMIT));
        assert_eq!(
            cap("enemies_ceiling"),
            Some(horde_core::progression::GAMEPLAY_ENEMY_CAP + 1)
        );

        // egui ceilings.
        assert_eq!(
            keys(object(&json, "/egui")),
            BTreeSet::from([
                "battlefield_primitives_max",
                "primitive_growth_52_to_400_max",
                "battlefield_vertices_max",
            ])
        );
        assert_eq!(
            whole(&json, "/egui/battlefield_primitives_max"),
            BATTLEFIELD_PRIMITIVES_MAX
        );
        assert_eq!(
            whole(&json, "/egui/primitive_growth_52_to_400_max"),
            PRIMITIVE_GROWTH_52_TO_400_MAX
        );
        assert_table_matches(
            BATTLEFIELD_VERTICES_MAX,
            object(&json, "/egui/battlefield_vertices_max"),
            "/egui/battlefield_vertices_max",
        );

        // CPU gate: the same scenes carry it on both sides, at the same values.
        let scenes = object(&json, "/scenes");
        assert_eq!(
            keys(scenes),
            keys(object(&json, "/run/frames")),
            "every gated scene has a frame count and vice versa"
        );
        let cpu_gated: BTreeSet<&str> = scenes
            .iter()
            .filter(|(_, scene)| scene.get("cpu_frame_p99_ms_max").is_some())
            .map(|(name, _)| name.as_str())
            .collect();
        assert_eq!(
            cpu_gated,
            CPU_GATED_SCENES.iter().copied().collect::<BTreeSet<_>>()
        );
        for name in CPU_GATED_SCENES {
            assert_eq!(
                number(&json, &format!("/scenes/{name}/cpu_frame_p99_ms_max")),
                CPU_FRAME_P99_MS_MAX,
                "{name}: CPU p99 gate"
            );
        }
        for name in scenes.keys() {
            assert_eq!(
                whole(&json, &format!("/scenes/{name}/frames_over_50ms_max")),
                FRAMES_OVER_50MS_MAX,
                "{name}: frames over 50 ms"
            );
        }
        assert_eq!(CPU_FRAME_P99_MS_MAX, 10.0);
        assert_eq!(FRAMES_OVER_50MS_MAX, 0);
    }

    #[test]
    fn perf_budget_baseline_is_wave0() {
        let json = budget();
        let wave0 = "/baseline/wave0";
        assert_eq!(
            json.pointer(&format!("{wave0}/commit")),
            Some(&"7bd5de8".into())
        );
        assert_eq!(
            json.pointer(&format!("{wave0}/profile")),
            Some(&"dev".into())
        );
        assert_eq!(json.pointer(&format!("{wave0}/vsync")), Some(&true.into()));
        assert_eq!(
            json.pointer(&format!("{wave0}/frame_report")),
            Some(&2.into())
        );
        assert_eq!(json.pointer("/baseline/v3"), Some(&Value::Null));
        let mean = |scene: &str, size: &str| {
            number(&json, &format!("{wave0}/interface_mean_ms/{scene}/{size}"))
        };
        assert_eq!(mean("progressed-peak", "normal"), 1.6128);
        assert_eq!(mean("progressed-peak", "small"), 1.4719);
        assert_eq!(mean("peak", "normal"), 1.1904);
        assert_eq!(mean("peak", "small"), 1.0752);

        let gate = |scene: &str, size: &str| {
            number(
                &json,
                &format!("/scenes/{scene}/interface_draw_mean_ms_max/{size}"),
            )
        };
        assert_eq!(gate("progressed-peak", "normal"), 1.129);
        assert_eq!(gate("progressed-peak", "small"), 1.030);
        for size in ["normal", "small"] {
            // The -30% gate is 0.70x wave 0 to the microsecond, never a re-baseline.
            let target = (0.70 * mean("progressed-peak", size) * 1000.0).round() / 1000.0;
            assert!(
                (gate("progressed-peak", size) - target).abs() < 1e-9,
                "progressed-peak {size}: gate is not 0.70x wave 0"
            );
            assert_eq!(gate("peak", size), mean("peak", size), "peak {size}");
        }
        assert_eq!(
            json.pointer("/scenes/progressed-peak/interface_total_not_above_baseline_v3"),
            Some(&true.into())
        );
    }

    #[test]
    fn perf_budget_integrated_gpu_is_pending_or_has_report() {
        let json = budget();
        let gpu = object(&json, "/hardware/integrated_gpu");
        assert_eq!(keys(gpu), BTreeSet::from(["status", "adapter", "report"]));
        assert_eq!(gpu["adapter"], "Intel(R) Arc(TM) Graphics");
        match gpu["status"].as_str() {
            Some("pending") => {
                assert_eq!(gpu["report"], Value::Null, "a pending run has no report");
                let row = ACCEPTANCE
                    .lines()
                    .find(|line| line.starts_with('|') && line.contains("Integrated-GPU run"))
                    .expect("docs/ACCEPTANCE.md has an Integrated-GPU run human-check row");
                assert!(
                    row.contains("pending human check")
                        && row.contains("Intel(R) Arc(TM) Graphics"),
                    "the ACCEPTANCE.md row logs the run as pending on the adapter: {row}"
                );
            }
            Some("measured") => assert!(
                gpu["report"]
                    .as_str()
                    .is_some_and(|report| !report.is_empty()),
                "a measured integrated-GPU run names its report"
            ),
            other => panic!("integrated_gpu.status must be pending or measured, got {other:?}"),
        }
    }

    /// 12.0 ms is only the pipelined-rendering decision threshold (ruling
    /// W-12): no gate outside `decisions` may carry it.
    #[test]
    fn twelve_ms_is_never_an_acceptance_gate() {
        fn walk(value: &Value, path: String, hits: &mut Vec<String>) {
            match value {
                Value::Object(map) => {
                    for (key, child) in map {
                        walk(child, format!("{path}/{key}"), hits);
                    }
                }
                Value::Array(items) => {
                    for (index, child) in items.iter().enumerate() {
                        walk(child, format!("{path}/{index}"), hits);
                    }
                }
                // Whole-number counts (loot_flyups 12) are not times.
                Value::Number(number)
                    if number.as_f64() == Some(12.0)
                        && (number.is_f64() || path.contains("ms")) =>
                {
                    hits.push(path)
                }
                _ => {}
            }
        }
        let mut hits = Vec::new();
        walk(&budget(), String::new(), &mut hits);
        hits.retain(|path| !path.starts_with("/decisions/"));
        assert!(hits.is_empty(), "12.0 outside decisions: {hits:?}");
    }
}
