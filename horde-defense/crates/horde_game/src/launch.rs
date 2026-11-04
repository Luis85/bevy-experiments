//! `LaunchConfig`: every command-line argument and `LASTLIGHT_*` variable the
//! native frontend honours, parsed and validated once in `main` before the App
//! exists. Nothing else in `horde_game` reads the process environment.
//! (Catalogue overrides are read by `horde_core::catalog`; they are listed here
//! only so startup can report their sources.)
use bevy::prelude::Resource;
use std::{ffi::OsString, path::PathBuf};

/// Default window size when `LASTLIGHT_REVIEW_SIZE` is absent or malformed.
pub const DEFAULT_WINDOW: (u32, u32) = (1440, 900);

/// What `main` should do with this process.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LaunchAction {
    #[default]
    Play,
    /// `--smoke`: headless self-check, report on stdout.
    Smoke,
    /// `--help`: print `HELP` and exit.
    Help,
}

/// Isolated review and measurement switches. Present only when at least one
/// review-only option is set; ordinary play and practice leave it `None`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReviewOptions {
    /// `LASTLIGHT_REVIEW_MOTION`: fixtures start unpaused.
    pub motion: bool,
    /// `LASTLIGHT_REVIEW_REDUCED`: force reduced effects in review fixtures.
    pub reduced: bool,
    /// `LASTLIGHT_REVIEW_CAPTURE`: automated screenshot path.
    pub capture: Option<PathBuf>,
    /// `LASTLIGHT_REVIEW_EXIT`: exit after the automated screenshot.
    pub exit: bool,
    /// `LASTLIGHT_CAPTURE_SEQUENCE`: fresh directory for a fixed-step frame export.
    pub sequence: Option<PathBuf>,
    /// `LASTLIGHT_REVIEW_SCALE`: UI scale applied by review fixtures.
    pub scale: Option<f32>,
    /// `LASTLIGHT_REVIEW_INPUT_SAVE`: copied generation for the `npc-stutter` fixture.
    pub input_save: Option<PathBuf>,
    /// `LASTLIGHT_PLAYER_MODEL_REVIEW_REPORT` (or the older `..._PARITY_REPORT`).
    pub model_report: Option<PathBuf>,
    pub perf: PerfOptions,
}

/// `LASTLIGHT_PERF_*`: opt-in frame evidence.
#[derive(Clone, Debug, PartialEq)]
pub struct PerfOptions {
    pub report: Option<PathBuf>,
    pub warmup_frames: u32,
    pub frames: u32,
    pub active_only: bool,
    pub cpu: Option<String>,
    pub audio: bool,
    pub npc_speed: Option<u32>,
}
impl Default for PerfOptions {
    fn default() -> Self {
        Self {
            report: None,
            warmup_frames: 90,
            frames: 300,
            active_only: false,
            cpu: None,
            audio: false,
            npc_speed: None,
        }
    }
}

#[derive(Resource, Clone, Debug, PartialEq)]
pub struct LaunchConfig {
    pub action: LaunchAction,
    /// `LASTLIGHT_REVIEW_SCREEN`: a staged scenario (review fixture or practice scene).
    pub scenario: Option<String>,
    /// `LASTLIGHT_PRACTICE`: the scenario is a resumable practice campaign.
    pub practice: bool,
    pub review: Option<ReviewOptions>,
    /// `LASTLIGHT_REVIEW_SIZE` (`WIDTHxHEIGHT`).
    pub window: (u32, u32),
    /// `LASTLIGHT_ASSET_DIR`.
    pub asset_dir: Option<PathBuf>,
    /// `LASTLIGHT_SAVE_DIR`, else `%LOCALAPPDATA%/LastlightHordeDefense`, else `saves`.
    pub save_directory: PathBuf,
    /// `LASTLIGHT_CAPTURE_DIR`: F12 screenshot folder.
    pub capture_dir: Option<PathBuf>,
    /// Catalogue name and override file, as `horde_core::catalog` resolves them.
    pub catalogs: Vec<(&'static str, Option<PathBuf>)>,
    /// Non-fatal problems found while parsing, logged once logging is up.
    pub warnings: Vec<String>,
}
impl Default for LaunchConfig {
    /// Ordinary play with no options: what hermetic tests construct.
    fn default() -> Self {
        Self {
            action: LaunchAction::Play,
            scenario: None,
            practice: false,
            review: None,
            window: DEFAULT_WINDOW,
            asset_dir: None,
            save_directory: PathBuf::from("saves"),
            capture_dir: None,
            catalogs: Vec::new(),
            warnings: Vec::new(),
        }
    }
}

pub const HELP: &str = "\
Lastlight: Horde Defense

USAGE:
    horde-defense [--smoke | --help]

OPTIONS:
    --smoke    Validate catalogues, run a headless campaign self-check and exit
    -h, --help Print this help and exit

ENVIRONMENT (empty values count as unset; flags accept 1/true/yes/on):
  Play
    LASTLIGHT_SAVE_DIR            Profile folder (default %LOCALAPPDATA%/LastlightHordeDefense)
    LASTLIGHT_ASSET_DIR           Asset folder (default ./assets beside the executable)
    LASTLIGHT_CAPTURE_DIR         F12 screenshot folder (default <save dir>/screenshots)
    LASTLIGHT_REVIEW_SCREEN       Staged scenario name (practice scene or review fixture)
    LASTLIGHT_PRACTICE            Flag: the scenario is a resumable practice campaign
    LASTLIGHT_REVIEW_SIZE         Window size WIDTHxHEIGHT (default 1440x900)
  Catalogue overrides (read by horde_core; listed at startup)
    LASTLIGHT_CONTENT, LASTLIGHT_WORLD, LASTLIGHT_SKILLS, LASTLIGHT_ENEMIES,
    LASTLIGHT_ORDNANCE, LASTLIGHT_PROGRESSION, LASTLIGHT_QUALITY, LASTLIGHT_BALANCE
  Review and measurement (builds with the `review` feature; otherwise ignored)
    LASTLIGHT_REVIEW_MOTION       Flag: review fixture starts unpaused
    LASTLIGHT_REVIEW_REDUCED      Flag: force reduced effects in the fixture
    LASTLIGHT_REVIEW_CAPTURE      Automated screenshot path
    LASTLIGHT_REVIEW_EXIT         Flag: exit after the automated screenshot
    LASTLIGHT_REVIEW_SCALE        UI scale for the fixture
    LASTLIGHT_REVIEW_INPUT_SAVE   Copied save generation (npc-stutter fixture)
    LASTLIGHT_CAPTURE_SEQUENCE    Fresh folder for a fixed-step motion export
                                  (needs a motion review scene; no practice or perf)
    LASTLIGHT_PLAYER_MODEL_REVIEW_REPORT  Player-model review manifest path
    LASTLIGHT_PERF_REPORT         Frame-evidence JSON path
    LASTLIGHT_PERF_WARMUP_FRAMES  0..=600 (default 90)
    LASTLIGHT_PERF_FRAMES         300..=3600 (default 300)
    LASTLIGHT_PERF_ACTIVE_ONLY    Flag: sample only unpaused battle frames
    LASTLIGHT_PERF_CPU            CPU label recorded in the report
    LASTLIGHT_PERF_AUDIO          Flag: keep audio in the perf-audio fixture
    LASTLIGHT_PERF_NPC_SPEED      Observation speed 1, 2 or 4 for perf fixtures
  Logging
    RUST_LOG                      Replaces the default log filter
";

/// Review-only variables. Any non-empty one activates `LaunchConfig::review`.
const REVIEW_VARIABLES: &[&str] = &[
    "LASTLIGHT_REVIEW_MOTION",
    "LASTLIGHT_REVIEW_REDUCED",
    "LASTLIGHT_REVIEW_CAPTURE",
    "LASTLIGHT_REVIEW_EXIT",
    "LASTLIGHT_REVIEW_SCALE",
    "LASTLIGHT_REVIEW_INPUT_SAVE",
    "LASTLIGHT_CAPTURE_SEQUENCE",
    "LASTLIGHT_PLAYER_MODEL_REVIEW_REPORT",
    "LASTLIGHT_PLAYER_MODEL_PARITY_REPORT",
    "LASTLIGHT_PERF_REPORT",
    "LASTLIGHT_PERF_WARMUP_FRAMES",
    "LASTLIGHT_PERF_FRAMES",
    "LASTLIGHT_PERF_ACTIVE_ONLY",
    "LASTLIGHT_PERF_CPU",
    "LASTLIGHT_PERF_AUDIO",
    "LASTLIGHT_PERF_NPC_SPEED",
];

/// Typed reads over one variable source. Empty Windows values are unset.
struct Env<F>(F);
impl<F: Fn(&str) -> Option<OsString>> Env<F> {
    fn string(&self, name: &str) -> Option<String> {
        (self.0)(name)
            .and_then(|v| v.into_string().ok())
            .filter(|v| !v.trim().is_empty())
    }
    fn path(&self, name: &str) -> Option<PathBuf> {
        nonempty_path((self.0)(name))
    }
    fn flag(&self, name: &str) -> bool {
        parse_flag(self.string(name).as_deref())
    }
}
fn nonempty_path(value: Option<OsString>) -> Option<PathBuf> {
    value.filter(|v| !v.is_empty()).map(PathBuf::from)
}
fn parse_flag(value: Option<&str>) -> bool {
    value.is_some_and(|v| {
        matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}
fn bounded_count(
    value: Option<&str>,
    default: u32,
    min: u32,
    max: u32,
    warnings: &mut Vec<String>,
) -> u32 {
    let Some(raw) = value else {
        return default;
    };
    match raw.parse::<u32>() {
        Ok(v) if (min..=max).contains(&v) => v,
        _ => {
            warnings.push(format!(
                "Ignoring out-of-range diagnostic count {raw:?} (allowed {min}..={max}); using {default}"
            ));
            default
        }
    }
}
fn parse_size(raw: &str) -> Option<(u32, u32)> {
    let (w, h) = raw.split_once('x')?;
    Some((w.parse().ok()?, h.parse().ok()?))
}

impl LaunchConfig {
    /// Parses this process's arguments and environment.
    pub fn from_process() -> Result<Self, String> {
        Self::parse(std::env::args_os().skip(1), |name| std::env::var_os(name))
    }

    /// Pure parser: `args` excludes the program name; `var` is the environment.
    pub fn parse(
        args: impl IntoIterator<Item = OsString>,
        var: impl Fn(&str) -> Option<OsString>,
    ) -> Result<Self, String> {
        let mut action = LaunchAction::Play;
        for arg in args {
            match arg.to_str() {
                Some("--smoke") => action = LaunchAction::Smoke,
                Some("--help" | "-h") => return Ok(Self::help()),
                _ => {
                    return Err(format!(
                        "Unknown argument {}. Run with --help for options.",
                        arg.to_string_lossy()
                    ));
                }
            }
        }
        let env = Env(var);
        let mut warnings = Vec::new();
        let window = match env.string("LASTLIGHT_REVIEW_SIZE") {
            Some(raw) => parse_size(&raw).unwrap_or_else(|| {
                warnings.push(format!(
                    "Ignoring LASTLIGHT_REVIEW_SIZE={raw:?}; expected WIDTHxHEIGHT"
                ));
                DEFAULT_WINDOW
            }),
            None => DEFAULT_WINDOW,
        };
        let save_directory = env.path("LASTLIGHT_SAVE_DIR").unwrap_or_else(|| {
            env.path("LOCALAPPDATA")
                .map(|p| p.join("LastlightHordeDefense"))
                .unwrap_or_else(|| PathBuf::from("saves"))
        });
        let scenario = env.string("LASTLIGHT_REVIEW_SCREEN");
        let practice = env.flag("LASTLIGHT_PRACTICE");
        let review_requested = REVIEW_VARIABLES
            .iter()
            .any(|name| env.string(name).is_some())
            || (scenario.is_some() && !practice);
        let review_requested = review_requested
            && {
                if !cfg!(feature = "review") {
                    warnings.push(
                    "Ignoring review, capture and performance variables: this build has no `review` feature (practice scenes need LASTLIGHT_PRACTICE=1)".into(),
                );
                }
                cfg!(feature = "review")
            };
        // Without review tooling a non-practice scenario would be a review fixture.
        let scenario = scenario.filter(|_| practice || cfg!(feature = "review"));
        let mut review = review_requested.then(|| ReviewOptions {
            motion: env.flag("LASTLIGHT_REVIEW_MOTION"),
            reduced: env.flag("LASTLIGHT_REVIEW_REDUCED"),
            capture: env.path("LASTLIGHT_REVIEW_CAPTURE"),
            exit: env.flag("LASTLIGHT_REVIEW_EXIT"),
            sequence: env.path("LASTLIGHT_CAPTURE_SEQUENCE"),
            scale: env
                .string("LASTLIGHT_REVIEW_SCALE")
                .and_then(|s| s.trim().parse().ok()),
            input_save: env.path("LASTLIGHT_REVIEW_INPUT_SAVE"),
            model_report: env
                .path("LASTLIGHT_PLAYER_MODEL_REVIEW_REPORT")
                .or_else(|| env.path("LASTLIGHT_PLAYER_MODEL_PARITY_REPORT")),
            perf: PerfOptions {
                report: env.path("LASTLIGHT_PERF_REPORT"),
                warmup_frames: bounded_count(
                    env.string("LASTLIGHT_PERF_WARMUP_FRAMES").as_deref(),
                    90,
                    0,
                    600,
                    &mut warnings,
                ),
                frames: bounded_count(
                    env.string("LASTLIGHT_PERF_FRAMES").as_deref(),
                    300,
                    300,
                    3600,
                    &mut warnings,
                ),
                active_only: env.flag("LASTLIGHT_PERF_ACTIVE_ONLY"),
                cpu: env.string("LASTLIGHT_PERF_CPU"),
                audio: env.flag("LASTLIGHT_PERF_AUDIO"),
                npc_speed: None,
            },
        });
        if let (Some(review), Some(raw)) = (&mut review, env.string("LASTLIGHT_PERF_NPC_SPEED")) {
            review.perf.npc_speed = Some(match raw.trim().parse() {
                Ok(speed @ (1 | 2 | 4)) => speed,
                _ => return Err("LASTLIGHT_PERF_NPC_SPEED must be 1, 2 or 4".into()),
            });
        }
        let config = Self {
            action,
            scenario,
            practice,
            review,
            window,
            asset_dir: env.path("LASTLIGHT_ASSET_DIR"),
            save_directory,
            capture_dir: env.path("LASTLIGHT_CAPTURE_DIR"),
            catalogs: horde_core::catalog::CATALOG_OVERRIDES
                .iter()
                .map(|&(_, name)| (name, env.path(name)))
                .collect(),
            warnings,
        };
        config.validate()?;
        Ok(config)
    }

    fn help() -> Self {
        Self {
            action: LaunchAction::Help,
            ..Self::default()
        }
    }

    /// Rejects combinations that would silently produce misleading evidence.
    fn validate(&self) -> Result<(), String> {
        if let Some(review) = &self.review
            && review.sequence.is_some()
            && (review.perf.report.is_some()
                || self.scenario.is_none()
                || self.practice
                || !review.motion)
        {
            return Err(
                "Animation export requires an isolated motion review and cannot be combined with performance measurement or a persistent campaign".into(),
            );
        }
        Ok(())
    }

    /// The isolated review options, if any; `None` in ordinary play and practice.
    pub fn review(&self) -> Option<&ReviewOptions> {
        self.review.as_ref()
    }
    /// An isolated (non-practice) review of `scene`.
    pub fn reviewing(&self, scene: &str) -> bool {
        self.review.is_some() && !self.practice && self.scenario.as_deref() == Some(scene)
    }
    pub fn motion_review(&self) -> bool {
        self.review().is_some_and(|r| r.motion)
    }
    pub fn capture_path(&self) -> Option<&PathBuf> {
        self.review().and_then(|r| r.capture.as_ref())
    }
    pub fn sequence_path(&self) -> Option<&PathBuf> {
        self.review().and_then(|r| r.sequence.as_ref())
    }
    pub fn perf(&self) -> Option<&PerfOptions> {
        self.review()
            .map(|r| &r.perf)
            .filter(|perf| perf.report.is_some())
    }
    pub fn log_directory(&self) -> PathBuf {
        self.save_directory.join("logs")
    }
    /// Every catalogue's source, for the startup diagnostic console.
    pub fn catalog_sources(&self) -> Vec<(&'static str, String)> {
        self.catalogs
            .iter()
            .map(|(name, path)| {
                (
                    *name,
                    path.as_ref().map_or_else(
                        || "bundled defaults".to_owned(),
                        |p| p.display().to_string(),
                    ),
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn parse(args: &[&str], vars: &[(&str, &str)]) -> Result<LaunchConfig, String> {
        let vars: HashMap<String, OsString> = vars
            .iter()
            .map(|(k, v)| ((*k).to_owned(), OsString::from(v)))
            .collect();
        LaunchConfig::parse(args.iter().map(OsString::from), |name| {
            vars.get(name).cloned()
        })
    }

    #[test]
    fn empty_and_false_opt_ins_never_enable_practice_or_performance() {
        for value in [
            None,
            Some(""),
            Some(" "),
            Some("0"),
            Some("false"),
            Some("off"),
        ] {
            assert!(!parse_flag(value));
        }
        for value in [Some("1"), Some(" true "), Some("YES"), Some("on")] {
            assert!(parse_flag(value));
        }
    }
    #[test]
    fn empty_report_paths_are_absent_not_current_directory() {
        assert!(nonempty_path(None).is_none());
        assert!(nonempty_path(Some(OsString::new())).is_none());
        assert_eq!(
            nonempty_path(Some(OsString::from("target/report.json"))),
            Some(PathBuf::from("target/report.json"))
        );
    }
    #[test]
    fn diagnostic_sample_budget_is_bounded_without_silently_shortening_measurements() {
        let mut w = Vec::new();
        assert_eq!(bounded_count(None, 300, 300, 3600, &mut w), 300);
        assert_eq!(bounded_count(Some("1800"), 300, 300, 3600, &mut w), 1800);
        assert!(w.is_empty());
        for bad in ["299", "3601", "-1", "NaN", "999999999999"] {
            assert_eq!(bounded_count(Some(bad), 300, 300, 3600, &mut w), 300);
        }
        assert_eq!(w.len(), 5, "every replaced value is reported");
        assert_eq!(bounded_count(Some("0"), 90, 0, 600, &mut w), 0);
    }
    #[test]
    fn ordinary_play_has_no_review_options_and_default_window() {
        let c = parse(&[], &[("LOCALAPPDATA", "C:/Users/x/AppData/Local")]).unwrap();
        assert_eq!(c.action, LaunchAction::Play);
        assert!(c.review.is_none() && c.scenario.is_none() && !c.practice);
        assert_eq!(c.window, DEFAULT_WINDOW);
        assert_eq!(
            c.save_directory,
            PathBuf::from("C:/Users/x/AppData/Local").join("LastlightHordeDefense")
        );
        assert_eq!(
            c.catalogs.len(),
            horde_core::catalog::CATALOG_OVERRIDES.len()
        );
    }
    #[test]
    fn save_dir_override_and_window_size_are_typed() {
        let c = parse(
            &[],
            &[
                ("LASTLIGHT_SAVE_DIR", "D:/saves"),
                ("LASTLIGHT_REVIEW_SIZE", "1800x1000"),
            ],
        )
        .unwrap();
        assert_eq!(c.save_directory, PathBuf::from("D:/saves"));
        assert_eq!(c.window, (1800, 1000));
        let c = parse(&[], &[("LASTLIGHT_REVIEW_SIZE", "wide")]).unwrap();
        assert_eq!(c.window, DEFAULT_WINDOW);
        assert_eq!(c.warnings.len(), 1);
    }
    #[test]
    fn practice_scene_is_play_not_review() {
        let c = parse(
            &[],
            &[
                ("LASTLIGHT_PRACTICE", "1"),
                ("LASTLIGHT_REVIEW_SCREEN", "construction"),
                ("LASTLIGHT_REVIEW_EXIT", ""),
            ],
        )
        .unwrap();
        assert!(c.practice);
        assert_eq!(c.scenario.as_deref(), Some("construction"));
        assert!(c.review.is_none());
    }
    #[test]
    fn arguments_select_smoke_or_help_and_reject_unknown_flags() {
        assert_eq!(
            parse(&["--smoke"], &[]).unwrap().action,
            LaunchAction::Smoke
        );
        assert_eq!(parse(&["--help"], &[]).unwrap().action, LaunchAction::Help);
        assert_eq!(parse(&["-h"], &[]).unwrap().action, LaunchAction::Help);
        assert!(parse(&["--bogus"], &[]).is_err());
        assert!(HELP.contains("LASTLIGHT_REVIEW_SCREEN"));
    }
    #[cfg(feature = "review")]
    #[test]
    fn review_scene_enables_review_options() {
        let c = parse(
            &[],
            &[
                ("LASTLIGHT_REVIEW_SCREEN", "damage"),
                ("LASTLIGHT_REVIEW_CAPTURE", "out.png"),
                ("LASTLIGHT_PERF_FRAMES", "1800"),
            ],
        )
        .unwrap();
        let review = c.review().unwrap();
        assert_eq!(review.capture, Some(PathBuf::from("out.png")));
        assert_eq!(review.perf.frames, 1800);
        assert!(c.reviewing("damage") && !c.reviewing("battle"));
        assert!(c.perf().is_none(), "no report path, no measurement");
    }
    #[cfg(feature = "review")]
    #[test]
    fn animation_export_requires_isolated_motion_review() {
        let base = [
            ("LASTLIGHT_CAPTURE_SEQUENCE", "frames"),
            ("LASTLIGHT_REVIEW_SCREEN", "gesture-throws"),
            ("LASTLIGHT_REVIEW_MOTION", "1"),
        ];
        assert!(parse(&[], &base).is_ok());
        for extra in [
            ("LASTLIGHT_PERF_REPORT", "perf.json"),
            ("LASTLIGHT_PRACTICE", "1"),
            ("LASTLIGHT_REVIEW_MOTION", "0"),
        ] {
            let mut vars = base.to_vec();
            vars.retain(|(k, _)| *k != extra.0);
            vars.push(extra);
            assert!(parse(&[], &vars).is_err(), "{extra:?}");
        }
        assert!(parse(&[], &base[..2]).is_err(), "motion flag required");
        assert!(parse(&[], &[base[0], base[2]]).is_err(), "scene required");
    }
    #[cfg(not(feature = "review"))]
    #[test]
    fn builds_without_review_warn_and_ignore_review_options() {
        for vars in [
            &[("LASTLIGHT_REVIEW_SCREEN", "damage")][..],
            &[("LASTLIGHT_PERF_REPORT", "perf.json")],
            &[("LASTLIGHT_REVIEW_SCALE", "1.2")],
            &[("LASTLIGHT_PLAYER_MODEL_REVIEW_REPORT", "m.json")],
        ] {
            let c = parse(&[], vars).unwrap();
            assert!(c.review.is_none() && c.scenario.is_none(), "{vars:?}");
            assert_eq!(c.warnings.len(), 1, "{vars:?}");
        }
        let c = parse(
            &[],
            &[
                ("LASTLIGHT_PRACTICE", "1"),
                ("LASTLIGHT_REVIEW_SCREEN", "construction"),
                ("LASTLIGHT_PERF_FRAMES", "900"),
            ],
        )
        .unwrap();
        assert_eq!(c.scenario.as_deref(), Some("construction"));
        assert!(c.practice && c.review.is_none());
    }
}
