//! Presentation performance budget. `docs/PERF_BUDGET.json` is the single gate
//! source the measurement script reads; these constants mirror it, and the
//! tests below hold the two equal in both directions, so neither side can
//! change alone. Nothing here touches the simulation.
#![cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "the budget is read by the tests and docs/PERF_BUDGET.json; the corpse sampling and cache consume the corpse caps, the egui ceilings are test gates"
    )
)]
use crate::{animation, ui};

/// -VsyncOff CPU frame (main + render world) p99 gate for the progressed-peak
/// and stress scenes, median of 3 runs. 12.0 ms is only the pipelined-rendering
/// decision threshold, never an acceptance gate.
pub const CPU_FRAME_P99_MS_MAX: f64 = 10.0;
/// Frames over 50 ms allowed in any gated scene, worst repeat.
pub const FRAMES_OVER_50MS_MAX: usize = 0;
/// W1-61, user ruling on gate 3 (2026-11-06, "Exclude driver stalls"): a
/// frame over `DRIVER_STALL_FRAME_MS_OVER` is an environmental driver stall
/// when the swap-chain acquire wait (present_wait, or the pipelined channel
/// wait while the render thread is in present) holds at least
/// `DRIVER_STALL_ACQUIRE_SHARE_MIN` of it, its named spans cover at least
/// `DRIVER_STALL_NAMED_COVERAGE_MIN` of it and its in-process work (main and
/// render CPU) stays below `DRIVER_STALL_IN_PROCESS_MS_BELOW`. The frame
/// report counts and lists every one (`frames_over_50ms_driver_stall`,
/// `driver_stalls`); `frames_over_50ms` gates the frames outside them, and
/// the pipelined decision reads its frames over 33 ms and VSync-on FPS and
/// p99 interval outside them too.
pub const DRIVER_STALL_FRAME_MS_OVER: f64 = 50.0;
pub const DRIVER_STALL_ACQUIRE_SHARE_MIN: f64 = 0.8;
pub const DRIVER_STALL_NAMED_COVERAGE_MIN: f64 = 0.95;
pub const DRIVER_STALL_IN_PROCESS_MS_BELOW: f64 = 50.0;
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

/// egui gates for the battlefield painter: the measured maximum x 1.25
/// (W1-51, `PERF_BUDGET.json` `evidence.egui_measured`), never above the
/// W1-32 hard ceilings (48 primitives; 40,000 / 72,000 vertices) and, by
/// ruling R22 (gates only tighten), never above the gate it replaces
/// (`egui_measured.previous.gates`). The W2 review re-measure on the W2-47
/// stress fixture (776075c), like the post-W1-69 one (4d599a5,
/// `egui_measured.w1_69`), reached 19 primitives in every scene, so
/// 19 x 1.25 gave 24. The W1-51 measurement (a93e340) reached 47, which
/// kept the 48 ceiling. The W5-01 render-pass split (world, grade,
/// vignette, cue pass) reaches 18 in every scene, so 18 x 1.25 = 22.5
/// tightens the gate to 23.
pub const BATTLEFIELD_PRIMITIVES_MAX: usize = 23;
/// Most extra primitives 400 enemies may cost over 52.
pub const PRIMITIVE_GROWTH_52_TO_400_MAX: usize = 2;
/// Battlefield vertex gates per measured scene. The W5-01 re-measure (cue
/// halos) reaches progressed-peak 39,444 and stress 56,345; neither gate
/// moves, by the same reasoning as before it. The W2 review re-measure
/// (776075c, the W2-47 stress fixture) reaches progressed-peak 38,524
/// (x 1.25 is above its 40,000 ceiling) and stress 52,325 with the corpse
/// caps full (x 1.25 = 65,406.25; post-W1-69, 4d599a5: 49,502). That is
/// above the 61,098 derived from the W1-51 measurement (a93e340, stress
/// 48,878), so under R22 (gates only tighten) the stress gate stays 61,098.
pub const BATTLEFIELD_VERTICES_MAX: &[(&str, usize)] =
    &[("progressed-peak", 40_000), ("stress", 61_098)];
/// egui texture uploads (`textures_delta.set` entries) allowed in battle once
/// warmup is over (W1-64): none. bevy_egui writes every partial font-atlas
/// update into `Assets<Image>`, so the render world re-extracts and
/// re-uploads the whole atlas. The frame report counts them as
/// `egui.texture_uploads`; `measure_performance.ps1` fails any scene whose
/// worst repeat is over it, and
/// `damage_popups_stop_uploading_font_texture_after_warmup` holds damage
/// popups to it headless.
pub const EGUI_TEXTURE_UPLOADS_AFTER_WARMUP_MAX: usize = 0;

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

        // egui ceilings.
        assert_eq!(
            keys(object(&json, "/egui")),
            BTreeSet::from([
                "battlefield_primitives_max",
                "primitive_growth_52_to_400_max",
                "battlefield_vertices_max",
                "texture_uploads_after_warmup_max",
            ])
        );
        assert_eq!(
            whole(&json, "/egui/texture_uploads_after_warmup_max"),
            EGUI_TEXTURE_UPLOADS_AFTER_WARMUP_MAX
        );
        assert_eq!(EGUI_TEXTURE_UPLOADS_AFTER_WARMUP_MAX, 0);
        // The cap is a live gate: the script compares every scene's worst
        // repeat of the report's upload count with it.
        for read in [
            "Get-Field $Budget @('egui', 'texture_uploads_after_warmup_max')",
            "Get-Field $r @('egui', 'texture_uploads', 'count')",
        ] {
            assert!(
                SCRIPT.contains(read),
                "measure_performance.ps1 reads {read}"
            );
        }
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

    /// W1-51: every egui gate is its measured maximum x 1.25, rounded up, and
    /// never above the W1-32 hard ceiling, so a re-measurement moves the
    /// recorded evidence and the gate together. R22 (wave-1 close): gates only
    /// tighten, so a re-derived gate is also never above the gate it replaces
    /// (`egui_measured.previous.gates`, the W1-51 gates from a93e340).
    #[test]
    fn egui_gates_are_measured_x_1_25_capped_at_the_w1_32_ceilings() {
        const CEILING_PRIMITIVES: usize = 48;
        const CEILING_VERTICES: &[(&str, usize)] =
            &[("progressed-peak", 40_000), ("stress", 72_000)];
        const W1_51_PRIMITIVES: usize = 48;
        const W1_51_VERTICES: &[(&str, usize)] = &[("progressed-peak", 40_000), ("stress", 61_098)];
        let json = budget();
        let measured = "/evidence/egui_measured";
        let factor = number(&json, &format!("{measured}/factor"));
        assert_eq!(factor, 1.25);
        let previous = format!("{measured}/previous/gates");
        assert_eq!(
            whole(&json, &format!("{previous}/battlefield_primitives_max")),
            W1_51_PRIMITIVES
        );
        assert_table_matches(
            W1_51_VERTICES,
            object(&json, &format!("{previous}/battlefield_vertices_max")),
            "previous/gates/battlefield_vertices_max",
        );
        let gate = |measured: usize, ceiling: usize, previous: usize| {
            ((measured as f64 * factor).ceil() as usize)
                .min(ceiling)
                .min(previous)
        };
        assert_eq!(
            whole(
                &json,
                &format!("{measured}/ceilings/battlefield_primitives_max")
            ),
            CEILING_PRIMITIVES
        );
        assert_table_matches(
            CEILING_VERTICES,
            object(
                &json,
                &format!("{measured}/ceilings/battlefield_vertices_max"),
            ),
            "ceilings/battlefield_vertices_max",
        );

        let primitives = whole(&json, &format!("{measured}/battlefield_primitives_max"));
        let by_scene = object(
            &json,
            &format!("{measured}/battlefield_primitives_by_scene"),
        );
        assert_eq!(
            by_scene.values().filter_map(Value::as_u64).max(),
            Some(primitives as u64),
            "the primitive maximum is the largest scene"
        );
        assert_eq!(
            BATTLEFIELD_PRIMITIVES_MAX,
            gate(primitives, CEILING_PRIMITIVES, W1_51_PRIMITIVES)
        );

        let vertices_by_scene = object(&json, &format!("{measured}/battlefield_vertices_by_scene"));
        assert_eq!(
            keys(by_scene),
            keys(vertices_by_scene),
            "the primitive and vertex tables cover the same measured cases"
        );
        let vertices = object(&json, &format!("{measured}/battlefield_vertices_max"));
        for (scene, max) in vertices {
            let covered = vertices_by_scene
                .iter()
                .filter(|(case, _)| *case == scene || case.starts_with(&format!("{scene}-")))
                .filter_map(|(_, value)| value.as_u64())
                .max();
            assert_eq!(
                covered,
                max.as_u64(),
                "{scene}: the vertex maximum is the largest of its measured cases"
            );
        }
        assert_eq!(
            keys(vertices),
            BATTLEFIELD_VERTICES_MAX
                .iter()
                .map(|(scene, _)| *scene)
                .collect()
        );
        for (scene, max) in BATTLEFIELD_VERTICES_MAX {
            let ceiling = CEILING_VERTICES
                .iter()
                .find(|(name, _)| name == scene)
                .map(|(_, ceiling)| *ceiling)
                .unwrap_or_else(|| panic!("{scene}: no W1-32 vertex ceiling"));
            let previous = W1_51_VERTICES
                .iter()
                .find(|(name, _)| name == scene)
                .map(|(_, gate)| *gate)
                .unwrap_or_else(|| panic!("{scene}: no W1-51 vertex gate"));
            let scene_max = whole(
                &json,
                &format!("{measured}/battlefield_vertices_max/{scene}"),
            );
            assert_eq!(
                *max,
                gate(scene_max, ceiling, previous),
                "{scene}: vertex gate"
            );
            assert!(*max <= ceiling, "{scene}: above the W1-32 ceiling");
        }
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
        // The target laptop's integrated adapter as wgpu names it (W1-68's
        // playtest-novsync-arc reports). Bevy picks an adapter only when the
        // lowercased request is a substring of its name, so a request missing
        // "Pro" silently runs on the NVIDIA GPU and every report is invalid.
        const ADAPTER: &str = "Intel(R) Arc(TM) Pro Graphics";
        assert_eq!(gpu["adapter"], ADAPTER);
        let command = format!("measure_performance.ps1 -Adapter \"{ADAPTER}\"");
        let plan_row = PLAN
            .lines()
            .find(|line| line.starts_with('|') && line.contains("Integrated-GPU run"))
            .expect("the plan's human checks have an Integrated-GPU run row");
        assert!(
            plan_row.contains(&command),
            "the plan row gives the protocol as {command}: {plan_row}"
        );
        match gpu["status"].as_str() {
            Some("pending") => {
                assert_eq!(gpu["report"], Value::Null, "a pending run has no report");
                let row = ACCEPTANCE
                    .lines()
                    .find(|line| line.starts_with('|') && line.contains("Integrated-GPU run"))
                    .expect("docs/ACCEPTANCE.md has an Integrated-GPU run human-check row");
                assert!(
                    row.contains("pending human check") && row.contains(&command),
                    "the ACCEPTANCE.md row logs the run as pending with {command}: {row}"
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

    const SCRIPT: &str = include_str!("../../../scripts/measure_performance.ps1");

    /// The `-Scene` values of `$sceneChoices` in
    /// `scripts/measure_performance.ps1` (the list its body validates
    /// `-Scene` against, so a bad value exits 3, not 1), without `all`.
    fn script_scenes(script: &str) -> BTreeSet<String> {
        let line = script
            .lines()
            .find(|line| line.trim_start().starts_with("$sceneChoices = @("))
            .expect("measure_performance.ps1 lists the -Scene values in $sceneChoices");
        let set = line
            .split("@(")
            .nth(1)
            .and_then(|rest| rest.split(')').next())
            .expect("a closed $sceneChoices list");
        set.split(',')
            .map(|value| value.trim().trim_matches(['\'', '"']).to_owned())
            .filter(|value| !value.is_empty() && value != "all")
            .collect()
    }

    /// W2-47 (R18): the battlefield egui gates are live in every perf run,
    /// not only headless. `measure_performance.ps1` compares each scene's
    /// worst repeat of the frame report's battlefield maxima with
    /// `egui.battlefield_primitives_max` and, where the scene has one, its
    /// `egui.battlefield_vertices_max` entry; every scene of the budget has a
    /// vertex gate or is a scene the headless measurement leaves ungated.
    #[test]
    fn measure_performance_gates_the_battlefield_primitives_and_vertices() {
        for read in [
            "Get-Field $Budget @('egui', 'battlefield_primitives_max')",
            "Get-Field $Budget @('egui', 'battlefield_vertices_max', $first.Scene)",
            "Get-Field $r @('egui', 'battlefield_primitives', 'max')",
            "Get-Field $r @('egui', 'battlefield_vertices', 'max')",
        ] {
            assert!(
                SCRIPT.contains(read),
                "measure_performance.ps1 reads {read}"
            );
        }
        let json = budget();
        let scenes = keys(object(&json, "/scenes"));
        for (scene, _) in BATTLEFIELD_VERTICES_MAX {
            assert!(
                scenes.contains(scene),
                "{scene}: a vertex gate needs a perf scene"
            );
        }
    }

    /// The script's `-Scene` set, the budget's gated scenes and the review
    /// registry's perf fixtures that run live (motion, no input save) are one
    /// set, so a scene can never be measured without a budget or gated
    /// without a staged fixture.
    #[test]
    fn perf_scenes_agree_across_script_budget_and_review_matrix() {
        use crate::review_scenes::{LIST_HEADER, REVIEW_SCENES, SceneKind, list_lines};
        let script = script_scenes(SCRIPT);
        let json = budget();
        let budget_scenes: BTreeSet<String> = keys(object(&json, "/scenes"))
            .into_iter()
            .map(str::to_owned)
            .collect();
        let matrix: BTreeSet<String> = REVIEW_SCENES
            .iter()
            .filter(|scene| scene.perf && scene.kind != SceneKind::NeedsInputSave)
            .map(|scene| scene.name.to_owned())
            .collect();
        assert_eq!(
            script, budget_scenes,
            "script -Scene vs PERF_BUDGET.json scenes"
        );
        assert_eq!(
            matrix, budget_scenes,
            "review perf scenes vs PERF_BUDGET.json scenes"
        );
        // The script keeps the --list-scenes rows whose fourth column is "true".
        assert_eq!(LIST_HEADER.split('\t').nth(3), Some("perf"));
        let listed = list_lines(true);
        for name in &matrix {
            let scene = REVIEW_SCENES
                .iter()
                .find(|scene| scene.name == name)
                .unwrap();
            assert!(
                scene.motion,
                "{name}: the script runs it live (LASTLIGHT_REVIEW_MOTION)"
            );
            let row = listed
                .iter()
                .find(|row| row.split('\t').next() == Some(name.as_str()))
                .unwrap_or_else(|| panic!("{name}: a review build lists it (--list-scenes)"));
            assert_eq!(
                row.split('\t').nth(3),
                Some("true"),
                "{name}: the --list-scenes perf column the script filters on: {row}"
            );
        }
    }

    /// Every way `baseline.v3` breaks the layout `measure_performance.ps1`
    /// reads for the interface_total gate: `{profile, vsync, present_mode,
    /// pipelined, power_plan, interface_total_mean_ms.<scene>.<size>}` for
    /// every scene with
    /// `interface_total_not_above_baseline_v3` and every size. Null (not yet
    /// recorded by W1-37) has no problems.
    fn v3_layout_problems(json: &Value) -> Vec<String> {
        let mut problems = Vec::new();
        let Some(v3) = json.pointer("/baseline/v3") else {
            return vec!["baseline.v3 is missing (null until W1-37 records it)".into()];
        };
        if v3.is_null() {
            return problems;
        }
        let profiles = object(json, "/profiles");
        match v3.get("profile").and_then(Value::as_str) {
            Some(profile) if profiles.contains_key(profile) => {}
            other => problems.push(format!("baseline.v3.profile {other:?} is not a profile")),
        }
        if !v3.get("vsync").is_some_and(Value::is_boolean) {
            problems.push("baseline.v3.vsync is not a bool".into());
        }
        // W1-37 review: the conditions the gate compares a run against.
        match (
            v3.get("vsync").and_then(Value::as_bool),
            v3.get("present_mode").and_then(Value::as_str),
        ) {
            (Some(true), Some("Fifo")) | (Some(false), Some("AutoNoVsync")) | (None, _) => {}
            (_, mode) => problems.push(format!(
                "baseline.v3.present_mode {mode:?} is not the mode its vsync requests"
            )),
        }
        if !v3.get("pipelined").is_some_and(Value::is_boolean) {
            problems.push("baseline.v3.pipelined is not a bool".into());
        }
        if !v3
            .get("power_plan")
            .and_then(Value::as_str)
            .is_some_and(|plan| !plan.trim().is_empty())
        {
            problems.push("baseline.v3.power_plan is not a plan name".into());
        }
        for (scene, gates) in object(json, "/scenes") {
            if gates.get("interface_total_not_above_baseline_v3") != Some(&Value::Bool(true)) {
                continue;
            }
            for size in object(json, "/sizes").keys() {
                let path = format!("/interface_total_mean_ms/{scene}/{size}");
                if !v3
                    .pointer(&path)
                    .and_then(Value::as_f64)
                    .is_some_and(|mean| mean > 0.0)
                {
                    problems.push(format!("baseline.v3{path} is not a positive number"));
                }
            }
        }
        problems
    }

    /// The interface_total gate never becomes a silent skip: the script and
    /// `PERF_BUDGET.json` agree on the `baseline.v3` layout, so W1-37 has to
    /// record it in this shape (the script reports any other shape as invalid
    /// evidence, exit 2).
    #[test]
    fn perf_budget_v3_baseline_has_the_layout_the_script_reads() {
        for read in [
            "Get-Field $Budget @('baseline', 'v3')",
            "Get-Field $v3 @('profile')",
            "Get-Field $v3 @('vsync')",
            "Get-Field $v3 @('interface_total_mean_ms', $first.Scene, $first.Size)",
            "Get-Field $v3 @('present_mode')",
            "Get-Field $v3 @('pipelined')",
            "Get-Field $v3 @('power_plan')",
        ] {
            assert!(
                SCRIPT.contains(read),
                "measure_performance.ps1 reads {read}"
            );
        }
        let json = budget();
        assert_eq!(v3_layout_problems(&json), Vec::<String>::new());

        let with_v3 = |v3: Value| {
            let mut json = budget();
            json["baseline"]["v3"] = v3;
            v3_layout_problems(&json)
        };
        let complete = serde_json::json!({
            "profile": "dev", "vsync": true,
            "present_mode": "Fifo", "pipelined": true, "power_plan": "Balanced",
            "interface_total_mean_ms": { "progressed-peak": { "normal": 2.1, "small": 1.9 } }
        });
        assert_eq!(with_v3(complete.clone()), Vec::<String>::new());
        let mut missing_size = complete.clone();
        missing_size["interface_total_mean_ms"]["progressed-peak"]
            .as_object_mut()
            .unwrap()
            .remove("small");
        assert_eq!(
            with_v3(missing_size),
            ["baseline.v3/interface_total_mean_ms/progressed-peak/small is not a positive number"]
        );
        let per_profile = serde_json::json!({ "dev": complete.clone() });
        assert_eq!(
            with_v3(per_profile).len(),
            6,
            "a per-profile layout is not read"
        );
        let mut no_vsync = complete.clone();
        no_vsync.as_object_mut().unwrap().remove("vsync");
        assert_eq!(with_v3(no_vsync), ["baseline.v3.vsync is not a bool"]);
        // The measured conditions are part of the layout (W1-37 review).
        let without = |key: &str| {
            let mut v3 = complete.clone();
            v3.as_object_mut().unwrap().remove(key);
            with_v3(v3)
        };
        assert_eq!(
            without("present_mode"),
            ["baseline.v3.present_mode None is not the mode its vsync requests"]
        );
        assert_eq!(
            without("pipelined"),
            ["baseline.v3.pipelined is not a bool"]
        );
        assert_eq!(
            without("power_plan"),
            ["baseline.v3.power_plan is not a plan name"]
        );
        let mut other_mode = complete;
        other_mode["present_mode"] = "AutoNoVsync".into();
        assert_eq!(with_v3(other_mode).len(), 1);
    }

    const PLAN: &str = include_str!("../../../docs/plans/2026-11-05-production-pass.md");
    const V3_SECTION: &str = "### 9.4 Frame report v3 baseline (W1-37)";

    /// W1-37 records frame report v3 on the unchanged renderer, in the wave-0
    /// profile and VSync state, for every budget scene and size: the
    /// interface_total anti-shift guard reads it, and the v2-compatible
    /// interface_draw beside it is compared with wave 0. A difference of more
    /// than 5% is investigated and written up in the plan's v3 section; the
    /// interface_draw gate itself never moves off wave 0.
    #[test]
    fn perf_budget_records_a_v3_baseline_for_every_scene_and_size() {
        let json = budget();
        let v3 = json
            .pointer("/baseline/v3")
            .filter(|v3| !v3.is_null())
            .expect("baseline.v3 is recorded (W1-37)");
        assert_eq!(v3_layout_problems(&json), Vec::<String>::new());
        assert_eq!(v3.get("profile"), json.pointer("/baseline/wave0/profile"));
        assert_eq!(v3.get("vsync"), json.pointer("/baseline/wave0/vsync"));
        assert_eq!(v3.get("frame_report"), Some(&3.into()));
        let commit = v3.get("commit").and_then(Value::as_str).unwrap_or("");
        assert!(
            commit.len() >= 7 && commit.chars().all(|c| c.is_ascii_hexdigit()),
            "baseline.v3.commit names the measured commit: {commit:?}"
        );
        assert!(
            whole(&json, "/baseline/v3/repeat") >= whole(&json, "/run/repeat_acceptance"),
            "baseline.v3 is a median of the acceptance repeat count"
        );

        // Every other measured profile and VSync state is recorded beside the
        // gate run, informational only (the script never reads `runs`).
        let measured: BTreeSet<&str> = object(&json, "/profiles")
            .iter()
            .filter(|(_, profile)| profile.get("dir").is_some())
            .map(|(name, _)| name.as_str())
            .collect();
        let mut recorded = BTreeSet::from([("dev".to_owned(), true)]);
        for (name, run) in object(&json, "/baseline/v3/runs") {
            let profile = run.get("profile").and_then(Value::as_str).unwrap_or("");
            let vsync = run.get("vsync").and_then(Value::as_bool);
            assert!(
                measured.contains(profile),
                "runs/{name}: profile {profile:?}"
            );
            let vsync = vsync.unwrap_or_else(|| panic!("runs/{name}: vsync is not a bool"));
            assert!(
                recorded.insert((profile.to_owned(), vsync)),
                "runs/{name}: {profile} VSync {vsync} is recorded twice"
            );
            let mut metrics = vec!["interface_total_mean_ms", "interface_draw_mean_ms"];
            if !vsync {
                metrics.push("cpu_frame_p99_ms");
            }
            for scene in keys(object(&json, "/scenes")) {
                for size in keys(object(&json, "/sizes")) {
                    for metric in &metrics {
                        let path = format!("/baseline/v3/runs/{name}/{metric}/{scene}/{size}");
                        assert!(number(&json, &path) > 0.0, "{path} is not positive");
                    }
                }
            }
        }
        let every: BTreeSet<(String, bool)> = measured
            .iter()
            .flat_map(|profile| {
                [
                    ((*profile).to_owned(), true),
                    ((*profile).to_owned(), false),
                ]
            })
            .collect();
        assert_eq!(
            recorded, every,
            "v3 covers every measured profile with VSync on and off"
        );

        let section = PLAN
            .split(V3_SECTION)
            .nth(1)
            .map(|rest| rest.split("\n## ").next().unwrap_or(rest))
            .expect("the plan records the v3 baseline in its own section");
        // The recorded conditions are the ones the plan states it ran under:
        // the W1-34 render plan defaults and the unchanged power plan.
        assert_eq!(v3.get("present_mode"), Some(&"Fifo".into()));
        assert_eq!(v3.get("pipelined"), Some(&true.into()));
        let power_plan = v3.get("power_plan").and_then(Value::as_str).unwrap_or("");
        assert!(
            section.contains("(Fifo, pipelined, frame latency 1)")
                && section.contains(&format!("power plan {power_plan} ")),
            "the plan's v3 section states Fifo, pipelined and the {power_plan:?} power plan"
        );
        let wave0 = object(&json, "/baseline/wave0/interface_mean_ms");
        for scene in keys(object(&json, "/scenes")) {
            for size in keys(object(&json, "/sizes")) {
                for metric in ["interface_total_mean_ms", "interface_draw_mean_ms"] {
                    let path = format!("/baseline/v3/{metric}/{scene}/{size}");
                    assert!(number(&json, &path) > 0.0, "{path} is not positive");
                }
                let Some(before) = wave0
                    .get(scene)
                    .and_then(|sizes| sizes.get(size))
                    .and_then(Value::as_f64)
                else {
                    continue;
                };
                let draw = number(
                    &json,
                    &format!("/baseline/v3/interface_draw_mean_ms/{scene}/{size}"),
                );
                let drift = (draw / before - 1.0).abs();
                if drift > 0.05 {
                    let note = format!("- Drift {scene} {size}:");
                    assert!(
                        section.lines().any(|line| line.starts_with(&note)),
                        "{scene} {size}: v3 interface_draw {draw} is {:.1}% off wave 0 {before}; \
                         the plan's v3 section needs a \"{note}\" investigation line",
                        drift * 100.0
                    );
                }
            }
        }
    }

    /// W2-47, ruling R29: `baseline.v3` is re-measured on the egui_pass and
    /// present_wait spans of review commit 7abb0ab, on the renderer W1-37
    /// measured (the same commit, only the two probe spans moved), so the
    /// interface_total anti-shift guard compares like spans with every run
    /// from 7abb0ab on. The W1-37 values stay verbatim under
    /// `baseline.v3.previous`, measured under the same conditions. Every
    /// re-measured median is in `measured_interface_total_mean_ms`, and
    /// `interface_total_mean_ms` (what the script gates) equals it, except
    /// that under R22 a gated cell never loosens: one measured above the
    /// previous value keeps that value and is listed in `kept_previous`.
    #[test]
    fn perf_budget_v3_baseline_is_re_measured_on_the_7abb0ab_spans() {
        let json = budget();
        assert_eq!(
            json.pointer("/baseline/v3/spans"),
            Some(&"7abb0ab".into()),
            "baseline.v3 names the probe spans it was measured on"
        );
        let previous = object(&json, "/baseline/v3/previous");
        assert_eq!(
            json.pointer("/baseline/v3/commit"),
            previous.get("commit"),
            "the re-measure keeps the W1-37 renderer"
        );
        assert_eq!(
            previous.get("evidence"),
            Some(&"target/perf/w1-37/dev-vsync-b".into())
        );
        for (size, mean) in [("normal", 4.6447), ("small", 4.5895)] {
            assert_eq!(
                number(
                    &json,
                    &format!(
                        "/baseline/v3/previous/interface_total_mean_ms/progressed-peak/{size}"
                    )
                ),
                mean,
                "previous keeps the W1-37 progressed-peak {size} value"
            );
        }
        for key in [
            "profile",
            "vsync",
            "present_mode",
            "pipelined",
            "power_plan",
        ] {
            assert_eq!(
                json.pointer(&format!("/baseline/v3/{key}")),
                previous.get(key),
                "{key}: the re-measure ran under the W1-37 conditions"
            );
        }
        let evidence = json
            .pointer("/baseline/v3/evidence")
            .and_then(Value::as_str)
            .unwrap_or("");
        assert!(
            evidence.starts_with("target/perf/w2-47/"),
            "baseline.v3.evidence is the W2-47 re-measure: {evidence:?}"
        );
        assert_eq!(
            keys(object(&json, "/baseline/v3/runs")),
            keys(object(&json, "/baseline/v3/previous/runs")),
            "every profile and VSync state is re-measured"
        );
        for (name, run) in object(&json, "/baseline/v3/runs") {
            let paths: Vec<&str> = match run.get("evidence") {
                Some(Value::String(path)) => vec![path.as_str()],
                Some(Value::Array(paths)) => paths.iter().filter_map(Value::as_str).collect(),
                _ => Vec::new(),
            };
            assert!(
                !paths.is_empty()
                    && paths
                        .iter()
                        .all(|path| path.starts_with("target/perf/w2-47/")),
                "runs/{name}: evidence from the W2-47 re-measure: {paths:?}"
            );
        }
        // The re-measured medians are recorded as measured; the gate reads
        // interface_total_mean_ms, which is the re-measure except where a
        // gated cell would loosen, and those cells are listed.
        let mut kept = BTreeSet::new();
        for (scene, gates) in object(&json, "/scenes") {
            let gated =
                gates.get("interface_total_not_above_baseline_v3") == Some(&Value::Bool(true));
            for size in keys(object(&json, "/sizes")) {
                let path = format!("interface_total_mean_ms/{scene}/{size}");
                let now = number(&json, &format!("/baseline/v3/{path}"));
                let measured = number(&json, &format!("/baseline/v3/measured_{path}"));
                let before = number(&json, &format!("/baseline/v3/previous/{path}"));
                if gated && measured > before {
                    kept.insert(path.clone());
                    assert_eq!(now, before, "{path}: R22 keeps the previous gate value");
                } else {
                    assert_eq!(now, measured, "{path}: the re-measured median");
                }
                if gated {
                    assert!(
                        now <= before,
                        "{path}: {now} loosens the gate above {before}"
                    );
                }
            }
        }
        let listed: BTreeSet<String> = json
            .pointer("/baseline/v3/kept_previous")
            .and_then(Value::as_array)
            .expect("baseline.v3.kept_previous lists the cells R22 kept")
            .iter()
            .filter_map(|cell| cell.as_str().map(str::to_owned))
            .collect();
        assert_eq!(listed, kept, "baseline.v3.kept_previous");
    }

    /// R30's load validity (`scripts/lib/MachineLoad.ps1`, InvalidAtPercent):
    /// a run with a CPU load sample at or above it is invalid evidence.
    const R30_INVALID_AT_PERCENT: u64 = 90;

    fn strings(value: Option<&Value>) -> Vec<&str> {
        value
            .and_then(Value::as_array)
            .map(|items| items.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default()
    }

    fn text<'a>(value: &'a Value, key: &str) -> &'a str {
        value.get(key).and_then(Value::as_str).unwrap_or("")
    }

    /// W2-47: the wave-2 re-gate on the new stress fixture is recorded in
    /// `acceptance.w2_47`. Each pass lists every invocation with its start,
    /// commit, executable and exit; a pass's `exit` is its last invocation's,
    /// and an invocation's exit follows from what it recorded (invalid groups
    /// rank above an over, as in measure_performance). Every invalid group is
    /// re-measured by a later invocation kept as evidence. A valid gated
    /// `over` is listed with its cell and repeats, and its pass is not clean:
    /// by R34 (plan 2.4) the over stands and W5-11 re-gates it (W2-47
    /// review). Done: dev and playtest `-VsyncOff`
    /// cover stress, exit 0 and have no stress over. The kept cpu_frame p99
    /// medians, interface_draw and interface_total means and worst frames
    /// over 50 ms meet the live gates, and the R31 A/B ran two different
    /// executables with B's median at most 1.05 x A's per size.
    #[test]
    fn perf_budget_records_the_w2_47_regate() {
        let json = budget();
        let path = "/acceptance/w2_47";
        let commits = strings(json.pointer(&format!("{path}/commits")));
        assert!(
            !commits.is_empty()
                && commits
                    .iter()
                    .all(|c| c.len() >= 7 && c.chars().all(|c| c.is_ascii_hexdigit())),
            "acceptance.w2_47.commits names every measured commit: {commits:?}"
        );
        let window = strings(json.pointer(&format!("{path}/window_utc")));
        assert_eq!(window.len(), 2, "acceptance.w2_47.window_utc: {window:?}");
        let note = json
            .pointer(&format!("{path}/note"))
            .and_then(Value::as_str)
            .unwrap_or("");
        assert!(
            note.contains("R34") && !note.contains("pending a ruling"),
            "acceptance.w2_47.note records the R34 answer, not an open ruling: {note:?}"
        );
        let repeat = whole(&json, "/run/repeat_acceptance");
        let passes = object(&json, &format!("{path}/passes"));
        for required in [
            "dev-novsync",
            "playtest-novsync",
            "dev-vsync",
            "playtest-vsync",
        ] {
            assert!(passes.contains_key(required), "passes/{required}");
        }
        let mut starts = Vec::new();
        let mut over_cells = std::collections::BTreeMap::new();
        for (name, pass) in passes {
            let command = text(pass, "command");
            let profile = command
                .split_whitespace()
                .skip_while(|word| *word != "-Profile")
                .nth(1)
                .unwrap_or("");
            let shas = strings(json.pointer(&format!("{path}/executables_sha256/{profile}")));
            assert!(
                !shas.is_empty(),
                "passes/{name}: {command:?} names a recorded profile"
            );
            let evidence = strings(pass.get("evidence"));
            assert!(
                !evidence.is_empty()
                    && evidence.iter().all(|p| p.starts_with("target/perf/w2-47/")),
                "passes/{name}: evidence under target/perf/w2-47/"
            );
            let kept = |invocation: &str| {
                evidence
                    .iter()
                    .any(|p| p.rsplit('/').next() == Some(invocation))
            };
            let mut order: Vec<(&str, &Value)> =
                object(&json, &format!("{path}/passes/{name}/invocations"))
                    .iter()
                    .map(|(key, record)| (key.as_str(), record))
                    .collect();
            order.sort_by_key(|(_, record)| text(record, "started_utc"));
            let mut overs = Vec::new();
            for (index, (invocation, record)) in order.iter().enumerate() {
                let at = format!("passes/{name}/invocations/{invocation}");
                let started = text(record, "started_utc");
                assert!(
                    started.len() == 20 && started.ends_with('Z'),
                    "{at}: started_utc {started:?}"
                );
                starts.push(started);
                let commit = text(record, "commit");
                assert!(
                    commits.contains(&commit),
                    "{at}: commit {commit:?} is in commits"
                );
                let sha = text(record, "executable_sha256");
                assert!(
                    shas.contains(&sha),
                    "{at}: executable {sha:?} is a recorded {profile} exe"
                );
                let invalid = strings(record.get("invalid_groups"));
                let over = record
                    .get("over")
                    .and_then(Value::as_array)
                    .map(Vec::as_slice)
                    .unwrap_or_default();
                // measure_performance ranks invalid evidence (2) above over budget (1).
                let expected = if !invalid.is_empty() {
                    2
                } else if !over.is_empty() {
                    1
                } else {
                    0
                };
                assert_eq!(
                    record.get("exit").and_then(Value::as_u64),
                    Some(expected),
                    "{at}: exit with invalid groups {invalid:?} and {} over cells",
                    over.len()
                );
                for group in &invalid {
                    assert!(
                        order[index + 1..].iter().any(|(later, r)| kept(later)
                            && strings(r.get("remeasures")).contains(group)),
                        "{at}: invalid group {group} is re-measured by a later invocation kept as evidence"
                    );
                }
                for cell in over {
                    let (scene, size) = (text(cell, "scene"), text(cell, "size"));
                    assert_eq!(
                        text(cell, "metric"),
                        "cpu_frame_p99_ms",
                        "{at}: over metric"
                    );
                    assert!(CPU_GATED_SCENES.contains(&scene), "{at}: {scene} is gated");
                    assert_eq!(
                        cell.get("limit").and_then(Value::as_f64),
                        Some(CPU_FRAME_P99_MS_MAX),
                        "{at}: the live gate"
                    );
                    let mut repeats: Vec<f64> = cell
                        .get("repeats")
                        .and_then(Value::as_array)
                        .map(|values| values.iter().filter_map(Value::as_f64).collect())
                        .unwrap_or_default();
                    assert_eq!(repeats.len(), repeat, "{at}: {scene}/{size} repeats");
                    repeats.sort_by(f64::total_cmp);
                    let median = cell.get("median").and_then(Value::as_f64);
                    assert_eq!(
                        median,
                        Some(repeats[repeat / 2]),
                        "{at}: {scene}/{size} median"
                    );
                    assert!(
                        median.is_some_and(|median| median > CPU_FRAME_P99_MS_MAX),
                        "{at}: {scene}/{size} is over"
                    );
                    let loads: Vec<u64> = cell
                        .get("load_max_percent")
                        .and_then(Value::as_array)
                        .map(|values| values.iter().filter_map(Value::as_u64).collect())
                        .unwrap_or_default();
                    assert!(
                        loads.len() == repeat
                            && loads.iter().all(|load| *load < R30_INVALID_AT_PERCENT),
                        "{at}: {scene}/{size} is valid under R30: loads {loads:?}"
                    );
                    overs.push(format!("{scene}/{size}"));
                }
            }
            let (last, last_record) = order
                .last()
                .unwrap_or_else(|| panic!("passes/{name} lists its invocations"));
            assert!(kept(last), "passes/{name}: {last} is kept as evidence");
            assert_eq!(
                pass.get("exit").and_then(Value::as_u64),
                last_record.get("exit").and_then(Value::as_u64),
                "passes/{name}: exit is its last invocation's"
            );
            assert_eq!(
                pass.get("exit").and_then(Value::as_u64),
                Some(0),
                "passes/{name} exits 0"
            );
            let clean = pass.get("gate_clean").and_then(Value::as_bool);
            if overs.is_empty() {
                assert_eq!(clean, Some(true), "passes/{name}: gate_clean");
            } else {
                // A valid over is not re-measured away: by R34 it stands and
                // W5-11 re-gates it.
                assert_eq!(clean, Some(false), "passes/{name}: a valid over {overs:?}");
                let reason = text(pass, "not_clean");
                assert!(
                    reason.contains("plan 2.5 row")
                        && reason.contains("R34")
                        && reason.contains("W5-11")
                        && !reason.contains("is answered")
                        && overs.iter().all(|cell| reason.contains(cell.as_str())),
                    "passes/{name}: not_clean names {overs:?}, the plan 2.5 row and its \
                     R34 answer: {reason:?}"
                );
            }
            over_cells.insert(name.as_str(), overs);
        }
        // Done: `-Scene stress -VsyncOff` in dev and playtest.
        for name in ["dev-novsync", "playtest-novsync"] {
            let command = text(&passes[name], "command");
            assert!(
                command.contains("-VsyncOff") && command.contains("-Scene all"),
                "passes/{name} covers stress with -VsyncOff: {command:?}"
            );
            assert!(
                over_cells[name]
                    .iter()
                    .all(|cell| !cell.starts_with("stress/")),
                "passes/{name}: no stress over"
            );
        }
        starts.sort_unstable();
        assert_eq!(
            window[0], starts[0],
            "window_utc starts at the first invocation"
        );
        assert!(
            starts.iter().all(|start| *start <= window[1]),
            "window_utc ends after every invocation starts"
        );
        let sizes = keys(object(&json, "/sizes"));
        for size in &sizes {
            for scene in ["peak", "progressed-peak"] {
                let draw = number(
                    &json,
                    &format!("{path}/interface_draw_mean_ms/{scene}/{size}"),
                );
                let limit = number(
                    &json,
                    &format!("/scenes/{scene}/interface_draw_mean_ms_max/{size}"),
                );
                assert!(
                    draw <= limit,
                    "{scene} {size}: interface_draw {draw} over {limit}"
                );
            }
            let total = number(
                &json,
                &format!("{path}/interface_total_mean_ms/progressed-peak/{size}"),
            );
            let limit = number(
                &json,
                &format!("/baseline/v3/interface_total_mean_ms/progressed-peak/{size}"),
            );
            assert!(
                total <= limit,
                "progressed-peak {size}: interface_total {total} over {limit}"
            );
        }
        for profile in ["dev", "playtest"] {
            for scene in CPU_GATED_SCENES {
                for size in &sizes {
                    let p99 = number(
                        &json,
                        &format!("{path}/cpu_frame_p99_ms/{profile}/{scene}/{size}"),
                    );
                    assert!(
                        p99 <= CPU_FRAME_P99_MS_MAX,
                        "{profile} {scene} {size}: cpu_frame p99 {p99}"
                    );
                }
            }
        }
        // The gate allows none, so at most the gate is exactly it.
        assert_eq!(
            whole(&json, &format!("{path}/frames_over_50ms_worst")),
            FRAMES_OVER_50MS_MAX,
            "frames over 50 ms outside driver stalls, worst repeat"
        );
        let ab = format!("{path}/interface_total_ab");
        let sha = |arm: &str| {
            json.pointer(&format!("{ab}/{arm}_sha256"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned()
        };
        assert!(
            sha("a").len() == 64 && sha("b").len() == 64 && sha("a") != sha("b"),
            "the A/B measured two executables"
        );
        let b_commit = json
            .pointer(&format!("{ab}/b_commit"))
            .and_then(Value::as_str)
            .unwrap_or("");
        assert!(
            commits.contains(&b_commit),
            "the A/B's B commit {b_commit:?} is in commits"
        );
        let tolerance = number(&json, &format!("{ab}/tolerance"));
        assert_eq!(tolerance, 1.05, "R31");
        for size in &sizes {
            let a = number(&json, &format!("{ab}/{size}/a_median"));
            let b = number(&json, &format!("{ab}/{size}/b_median"));
            assert!(
                a > 0.0 && b <= tolerance * a,
                "{size}: B {b} over {tolerance} x A {a}"
            );
        }
    }

    const VALIDATION: &str = include_str!("../../../docs/VALIDATION.md");

    /// W1-61, the user ruling on gate 3 ("Exclude driver stalls"): the
    /// thresholds the frame report classifies with are the ones the budget
    /// records, the script gates frames_over_50ms on the frames outside the
    /// stalls, and the plan and VALIDATION.md quote the ruling.
    #[test]
    fn driver_stall_rule_matches_the_ruling() {
        let json = budget();
        let path = "/driver_stall";
        assert_eq!(
            keys(object(&json, path)),
            BTreeSet::from([
                "ruling",
                "ruled",
                "rule",
                "frame_ms_over",
                "acquire_share_min",
                "named_coverage_min",
                "in_process_ms_below",
                "report",
            ])
        );
        assert_eq!(
            json.pointer("/driver_stall/ruling"),
            Some(&"Exclude driver stalls".into())
        );
        let rule = json
            .pointer("/driver_stall/rule")
            .and_then(Value::as_str)
            .expect("driver_stall.rule is text");
        for value in [
            (
                "/driver_stall/frame_ms_over",
                DRIVER_STALL_FRAME_MS_OVER,
                "over 50 ms",
            ),
            (
                "/driver_stall/acquire_share_min",
                DRIVER_STALL_ACQUIRE_SHARE_MIN,
                "at least 80%",
            ),
            (
                "/driver_stall/in_process_ms_below",
                DRIVER_STALL_IN_PROCESS_MS_BELOW,
                "below 50 ms",
            ),
        ] {
            assert_eq!(number(&json, value.0), value.1, "{}", value.0);
            assert!(rule.contains(value.2), "the rule text says {:?}", value.2);
        }
        assert_eq!(
            number(&json, "/driver_stall/named_coverage_min"),
            DRIVER_STALL_NAMED_COVERAGE_MIN
        );
        assert_eq!(DRIVER_STALL_FRAME_MS_OVER, 50.0);
        assert_eq!(DRIVER_STALL_ACQUIRE_SHARE_MIN, 0.8);
        assert_eq!(DRIVER_STALL_IN_PROCESS_MS_BELOW, 50.0);
        assert_eq!(DRIVER_STALL_NAMED_COVERAGE_MIN, 0.95);
        for (key, field) in [
            ("count", "frames_over_50ms_driver_stall"),
            ("list", "driver_stalls.frames"),
            ("gated", "driver_stalls.outside"),
        ] {
            assert_eq!(
                json.pointer(&format!("/driver_stall/report/{key}")),
                Some(&field.into())
            );
        }
        for read in [
            "Get-Field $r @('driver_stalls', 'outside', 'frames_over_50ms')",
            "Get-Field $r @('frames_over_50ms_driver_stall')",
            "Get-Field $r @('driver_stalls', 'frames')",
        ] {
            assert!(
                SCRIPT.contains(read),
                "measure_performance.ps1 reads {read}"
            );
        }
        assert!(
            !SCRIPT.contains("Get-Field $r @('frames_over_50ms') } 'frames_over_50ms'"),
            "the frames_over_50ms gate reads the frames outside driver stalls"
        );
        let quoted = "Exclude driver stalls";
        for (name, text) in [("the plan", PLAN), ("VALIDATION.md", VALIDATION)] {
            assert!(text.contains(quoted), "{name} quotes the ruling");
        }
        let section_9_4 = PLAN
            .split(V3_SECTION)
            .nth(1)
            .map(|rest| rest.split("\n### 9.5").next().unwrap_or(rest))
            .expect("the plan has section 9.4");
        assert!(
            section_9_4.contains(quoted),
            "plan section 9.4 quotes the ruling"
        );
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

    /// The measured pipelined-rendering decision (W1-61). The rule (design
    /// perf-architecture, "Pipelined rendering default"): switch the default
    /// OFF only if, with pipelining off, every -VsyncOff progressed-peak and
    /// stress cpu_frame p99 (median of 3, both profiles, both sizes) is at most
    /// 12.0 ms with 0 frames over 33 ms, and the VSync-on runs keep a mean FPS
    /// of at least 59.5, a p99 frame interval of at most 17.7 ms and 0 frames
    /// over 50 ms. Otherwise keep it ON. Per the user ruling on gate 3 the
    /// frame counts, FPS and p99 interval are those outside driver stalls. The
    /// recorded off-arm numbers are re-judged here, so `PIPELINED_DEFAULT`, the
    /// recorded default and the recorded evidence cannot drift apart.
    #[test]
    fn pipelined_default_matches_recorded_decision() {
        let json = budget();
        let path = "/decisions/pipelined_rendering";
        let decision = json
            .pointer(path)
            .filter(|decision| decision.is_object())
            .unwrap_or_else(|| {
                panic!("decisions.pipelined_rendering is not recorded (needs >= 12 playtest runs)")
            });
        assert_eq!(
            keys(object(&json, path)),
            BTreeSet::from([
                "default",
                "rule",
                "commit",
                "thresholds",
                "runs",
                "off",
                "on",
                "evidence",
            ]),
            "{path} keys"
        );
        let default = decision["default"]
            .as_bool()
            .expect("decisions.pipelined_rendering.default is a bool");
        let rule = decision["rule"].as_str().unwrap_or("");
        assert!(
            rule.contains("12.0"),
            "the rule text names the 12.0 ms threshold"
        );
        assert!(
            rule.contains("driver_stalls.outside"),
            "the rule text says its counts exclude driver stalls"
        );
        let commit = decision["commit"].as_str().unwrap_or("");
        assert!(
            commit.len() >= 7 && commit.chars().all(|c| c.is_ascii_hexdigit()),
            "the decision names the measured commit: {commit:?}"
        );
        // The rule's thresholds, pinned: they are the decision, not tunables.
        let thresholds = format!("{path}/thresholds");
        assert_eq!(
            keys(object(&json, &thresholds)),
            BTreeSet::from([
                "cpu_frame_p99_ms_max",
                "frames_over_33ms_max",
                "vsync_on_mean_fps_min",
                "vsync_on_p99_frame_ms_max",
                "vsync_on_frames_over_50ms_max",
            ])
        );
        assert_eq!(
            number(&json, &format!("{thresholds}/cpu_frame_p99_ms_max")),
            12.0
        );
        assert_eq!(
            whole(&json, &format!("{thresholds}/frames_over_33ms_max")),
            0
        );
        assert_eq!(
            number(&json, &format!("{thresholds}/vsync_on_mean_fps_min")),
            59.5
        );
        assert_eq!(
            number(&json, &format!("{thresholds}/vsync_on_p99_frame_ms_max")),
            17.7
        );
        assert_eq!(
            whole(
                &json,
                &format!("{thresholds}/vsync_on_frames_over_50ms_max")
            ),
            0
        );

        // At least 12 valid runs per arm (2 scenes x 2 sizes x 3 repeats) for
        // playtest and dev with VSync off, and for the VSync-on pair.
        for group in ["playtest", "dev", "vsync_on"] {
            for arm in ["on", "off"] {
                let runs = whole(&json, &format!("{path}/runs/{group}/{arm}"));
                assert!(
                    runs >= 12,
                    "{group} pipelined {arm}: {runs} runs, need >= 12"
                );
            }
        }
        let evidence = decision["evidence"].as_array().expect("evidence is a list");
        assert!(
            !evidence.is_empty()
                && evidence.iter().all(|item| item
                    .as_str()
                    .is_some_and(|dir| dir.starts_with("target/perf/"))),
            "evidence names the target/perf passes"
        );

        // Both arms carry every cell; only the off arm decides.
        let mut off_qualifies = true;
        for arm in ["off", "on"] {
            for profile in ["playtest", "dev"] {
                for scene in CPU_GATED_SCENES {
                    for size in ["normal", "small"] {
                        let at = format!("{path}/{arm}/vsync_off/{profile}/{scene}/{size}");
                        let p99 = number(&json, &format!("{at}/cpu_frame_p99_ms_median"));
                        let over_33 = whole(&json, &format!("{at}/frames_over_33ms_worst"));
                        whole(&json, &format!("{at}/driver_stalls"));
                        if arm == "off" {
                            off_qualifies &= p99 <= 12.0 && over_33 == 0;
                        }
                    }
                }
            }
            for scene in CPU_GATED_SCENES {
                for size in ["normal", "small"] {
                    let at = format!("{path}/{arm}/vsync_on/{scene}/{size}");
                    let fps = number(&json, &format!("{at}/mean_fps_median"));
                    let p99 = number(&json, &format!("{at}/p99_frame_ms_median"));
                    let over_50 = whole(&json, &format!("{at}/frames_over_50ms_worst"));
                    whole(&json, &format!("{at}/driver_stalls"));
                    if arm == "off" {
                        off_qualifies &= fps >= 59.5 && p99 <= 17.7 && over_50 == 0;
                    }
                }
            }
        }
        assert_eq!(
            default, !off_qualifies,
            "the recorded default does not follow the rule from the recorded off-arm numbers"
        );
        assert_eq!(
            crate::render_plan::PIPELINED_DEFAULT,
            default,
            "PIPELINED_DEFAULT differs from decisions.pipelined_rendering.default"
        );
    }
}
