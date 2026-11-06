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
/// W1-32 hard ceilings (48 primitives; 40,000 / 72,000 vertices). Live runs
/// reach 47 primitives (stress), so 47 x 1.25 keeps the 48 ceiling.
pub const BATTLEFIELD_PRIMITIVES_MAX: usize = 48;
/// Most extra primitives 400 enemies may cost over 52.
pub const PRIMITIVE_GROWTH_52_TO_400_MAX: usize = 2;
/// Battlefield vertex gates per measured scene: progressed-peak measured
/// 37,940 (x 1.25 is above its 40,000 ceiling), stress 48,878 with the corpse
/// caps full (x 1.25 = 61,097.5).
pub const BATTLEFIELD_VERTICES_MAX: &[(&str, usize)] =
    &[("progressed-peak", 40_000), ("stress", 61_098)];
/// egui texture uploads (`textures_delta.set` entries) allowed in battle once
/// warmup is over (W1-64): none. bevy_egui writes every partial font-atlas
/// update into `Assets<Image>`, so the render world re-extracts and
/// re-uploads the whole atlas. The frame report counts them as
/// `egui.texture_uploads`; `damage_popups_stop_uploading_font_texture_after_warmup`
/// holds damage popups to it headless.
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
                "texture_uploads_after_warmup_max",
            ])
        );
        assert_eq!(
            whole(&json, "/egui/texture_uploads_after_warmup_max"),
            EGUI_TEXTURE_UPLOADS_AFTER_WARMUP_MAX
        );
        assert_eq!(EGUI_TEXTURE_UPLOADS_AFTER_WARMUP_MAX, 0);
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
    /// recorded evidence and the gate together.
    #[test]
    fn egui_gates_are_measured_x_1_25_capped_at_the_w1_32_ceilings() {
        const CEILING_PRIMITIVES: usize = 48;
        const CEILING_VERTICES: &[(&str, usize)] =
            &[("progressed-peak", 40_000), ("stress", 72_000)];
        let json = budget();
        let measured = "/evidence/egui_measured";
        let factor = number(&json, &format!("{measured}/factor"));
        assert_eq!(factor, 1.25);
        let gate = |measured: usize, ceiling: usize| {
            ((measured as f64 * factor).ceil() as usize).min(ceiling)
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
            gate(primitives, CEILING_PRIMITIVES)
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
            let scene_max = whole(
                &json,
                &format!("{measured}/battlefield_vertices_max/{scene}"),
            );
            assert_eq!(*max, gate(scene_max, ceiling), "{scene}: vertex gate");
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
    /// reads for the interface_total gate: `{profile, vsync,
    /// interface_total_mean_ms.<scene>.<size>}` for every scene with
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
            4,
            "a per-profile layout is not read"
        );
        let mut no_vsync = complete;
        no_vsync.as_object_mut().unwrap().remove("vsync");
        assert_eq!(with_v3(no_vsync), ["baseline.v3.vsync is not a bool"]);
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
