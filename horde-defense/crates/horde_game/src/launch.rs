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
    /// `--list-scenes`: print the review scene registry as TSV and exit.
    ListScenes,
    /// `--replay <path>` (`dev` feature): headless replay of a recording.
    #[cfg(feature = "dev")]
    Replay,
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
    /// `LASTLIGHT_PERF_PROFILE`: build profile label recorded in the report.
    pub profile_label: Option<String>,
    pub audio: bool,
    pub npc_speed: Option<u32>,
    /// `LASTLIGHT_PERF_PRESENT_MODE`: wins over the VSync and Low latency settings.
    pub present_mode: Option<PresentModeOverride>,
    /// `LASTLIGHT_PERF_PIPELINED` (0|1): wins over Low latency and the default.
    pub pipelined: Option<bool>,
}
impl Default for PerfOptions {
    fn default() -> Self {
        Self {
            report: None,
            warmup_frames: 90,
            frames: 300,
            active_only: false,
            cpu: None,
            profile_label: None,
            audio: false,
            npc_speed: None,
            present_mode: None,
            pipelined: None,
        }
    }
}

/// `LASTLIGHT_PERF_PRESENT_MODE` values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentModeOverride {
    Fifo,
    Mailbox,
    Immediate,
    AutoNoVsync,
}
impl PresentModeOverride {
    fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "fifo" => Ok(Self::Fifo),
            "mailbox" => Ok(Self::Mailbox),
            "immediate" => Ok(Self::Immediate),
            "autonovsync" => Ok(Self::AutoNoVsync),
            _ => Err(
                "LASTLIGHT_PERF_PRESENT_MODE must be fifo, mailbox, immediate or autonovsync"
                    .into(),
            ),
        }
    }
    pub fn present_mode(self) -> bevy::window::PresentMode {
        use bevy::window::PresentMode;
        match self {
            Self::Fifo => PresentMode::Fifo,
            Self::Mailbox => PresentMode::Mailbox,
            Self::Immediate => PresentMode::Immediate,
            Self::AutoNoVsync => PresentMode::AutoNoVsync,
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
    /// Where an isolated review logs when `LASTLIGHT_SAVE_DIR` is absent: the
    /// per-process temp review profile, so review launches never write or prune
    /// logs in the player's profile. `None` for play, practice and explicit saves.
    pub log_directory_override: Option<PathBuf>,
    /// `LASTLIGHT_CAPTURE_DIR`: F12 screenshot folder.
    pub capture_dir: Option<PathBuf>,
    /// Catalogue name and override file, as `horde_core::catalog` resolves them.
    pub catalogs: Vec<(&'static str, Option<PathBuf>)>,
    /// Non-fatal problems found while parsing, logged once logging is up.
    pub warnings: Vec<String>,
    /// `--record <path>` or `LASTLIGHT_RECORD` (`dev` feature): command recording
    /// that starts with the session.
    #[cfg(feature = "dev")]
    pub record: Option<PathBuf>,
    /// `--replay <path>` (`dev` feature): the recording to replay headless.
    #[cfg(feature = "dev")]
    pub replay: Option<PathBuf>,
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
            log_directory_override: None,
            capture_dir: None,
            catalogs: Vec::new(),
            warnings: Vec::new(),
            #[cfg(feature = "dev")]
            record: None,
            #[cfg(feature = "dev")]
            replay: None,
        }
    }
}

pub const HELP: &str = "\
Lastlight: Horde Defense

USAGE:
    horde-defense [--smoke | --help | --list-scenes]
    horde-defense [--record <file>]        (`dev` feature)
    horde-defense --replay <file>          (`dev` feature)

OPTIONS:
    --smoke    Validate catalogues, run a headless campaign self-check and exit
    -h, --help Print this help and exit
    --list-scenes  Print every LASTLIGHT_REVIEW_SCREEN name as TSV and exit
                   (a build without `review` lists only practice scenes)
    --record <file>  Record commands and per-tick input to a JSONL file (F7 toggles in game)
    --replay <file>  Replay a recording headless, print the final state digest and exit

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
    LASTLIGHT_PERF_PROFILE        Build profile label recorded in the report
    LASTLIGHT_PERF_AUDIO          Flag: keep audio in the perf-audio fixture
    LASTLIGHT_PERF_NPC_SPEED      Observation speed 1, 2 or 4 for perf fixtures
    LASTLIGHT_PERF_PRESENT_MODE   fifo, mailbox, immediate or autonovsync; wins over
                                  the VSync and Low latency settings
    LASTLIGHT_PERF_PIPELINED      0 or 1: pipelined rendering, wins over Low latency
  Developer tools (builds with the `dev` feature; otherwise rejected or ignored)
    LASTLIGHT_RECORD              Same as --record
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
    "LASTLIGHT_PERF_PROFILE",
    "LASTLIGHT_PERF_AUDIO",
    "LASTLIGHT_PERF_NPC_SPEED",
    "LASTLIGHT_PERF_PRESENT_MODE",
    "LASTLIGHT_PERF_PIPELINED",
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
        #[cfg(feature = "dev")]
        let (mut record, mut replay) = (None, None);
        let mut args = args.into_iter();
        // Without `dev` no flag consumes a value, so only the dev build needs `next()`.
        #[cfg_attr(not(feature = "dev"), allow(clippy::while_let_on_iterator))]
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--smoke") => action = LaunchAction::Smoke,
                Some("--help" | "-h") => return Ok(Self::help()),
                Some("--list-scenes") => {
                    return Ok(Self {
                        action: LaunchAction::ListScenes,
                        ..Self::default()
                    });
                }
                Some(flag @ ("--record" | "--replay")) => {
                    #[cfg(feature = "dev")]
                    {
                        let path = args
                            .next()
                            .filter(|path| !path.is_empty())
                            .ok_or_else(|| format!("{flag} needs a file path"))?;
                        if flag == "--record" {
                            record = Some(PathBuf::from(path));
                        } else {
                            replay = Some(PathBuf::from(path));
                            action = LaunchAction::Replay;
                        }
                    }
                    #[cfg(not(feature = "dev"))]
                    return Err(format!("{flag} needs a build with the `dev` feature."));
                }
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
        let explicit_save_directory = env.path("LASTLIGHT_SAVE_DIR");
        let save_directory = explicit_save_directory.clone().unwrap_or_else(|| {
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
                profile_label: env.string("LASTLIGHT_PERF_PROFILE"),
                audio: env.flag("LASTLIGHT_PERF_AUDIO"),
                npc_speed: None,
                present_mode: None,
                pipelined: None,
            },
        });
        if let (Some(review), Some(raw)) = (&mut review, env.string("LASTLIGHT_PERF_NPC_SPEED")) {
            review.perf.npc_speed = Some(match raw.trim().parse() {
                Ok(speed @ (1 | 2 | 4)) => speed,
                _ => return Err("LASTLIGHT_PERF_NPC_SPEED must be 1, 2 or 4".into()),
            });
        }
        if let (Some(review), Some(raw)) = (&mut review, env.string("LASTLIGHT_PERF_PRESENT_MODE"))
        {
            review.perf.present_mode = Some(PresentModeOverride::parse(&raw)?);
        }
        if let (Some(review), Some(raw)) = (&mut review, env.string("LASTLIGHT_PERF_PIPELINED")) {
            review.perf.pipelined = Some(match raw.trim() {
                "0" => false,
                "1" => true,
                _ => return Err("LASTLIGHT_PERF_PIPELINED must be 0 or 1".into()),
            });
        }
        // An isolated review stages a throwaway profile; without an explicit save
        // directory its logs go there too, never into the player's profile.
        let log_directory_override = (review.is_some()
            && !practice
            && scenario.is_some()
            && explicit_save_directory.is_none())
        .then(|| {
            crate::profile::profile_directory(save_directory.clone(), scenario.as_deref(), false)
                .join("logs")
        });
        #[cfg(feature = "dev")]
        let record = record.or_else(|| env.path("LASTLIGHT_RECORD"));
        #[cfg(not(feature = "dev"))]
        if env.string("LASTLIGHT_RECORD").is_some() {
            warnings.push("Ignoring LASTLIGHT_RECORD: this build has no `dev` feature".into());
        }
        let config = Self {
            action,
            scenario,
            practice,
            review,
            window,
            asset_dir: env.path("LASTLIGHT_ASSET_DIR"),
            save_directory,
            log_directory_override,
            capture_dir: env.path("LASTLIGHT_CAPTURE_DIR"),
            catalogs: horde_core::catalog::CATALOG_OVERRIDES
                .iter()
                .map(|&(_, name)| (name, env.path(name)))
                .collect(),
            warnings,
            #[cfg(feature = "dev")]
            record,
            #[cfg(feature = "dev")]
            replay,
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

    /// Rejects unknown scenes and combinations that would silently produce
    /// misleading evidence.
    fn validate(&self) -> Result<(), String> {
        // A build accepts exactly the scenes its --list-scenes lists.
        if let Some(scene) = &self.scenario
            && !crate::review_scenes::accepts(scene, cfg!(feature = "review"))
        {
            return Err(format!(
                "Unknown review scene \"{scene}\"; run --list-scenes"
            ));
        }
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
        #[cfg(feature = "dev")]
        if self.replay.is_some() && self.record.is_some() {
            return Err("--replay cannot be combined with recording".into());
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
    /// The review override when one is set, otherwise `<save directory>/logs`.
    pub fn log_directory(&self) -> PathBuf {
        self.log_directory_override
            .clone()
            .unwrap_or_else(|| self.save_directory.join("logs"))
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
    #[test]
    fn list_scenes_is_a_launch_action() {
        assert_eq!(
            parse(&["--list-scenes"], &[]).unwrap().action,
            LaunchAction::ListScenes
        );
        // Listing never depends on (or validates) the scene being launched.
        assert_eq!(
            parse(
                &["--list-scenes"],
                &[("LASTLIGHT_REVIEW_SCREEN", "camp-hmoe")]
            )
            .unwrap()
            .action,
            LaunchAction::ListScenes
        );
        assert!(HELP.contains("--list-scenes"));
    }
    #[test]
    fn unknown_practice_scene_is_rejected() {
        let practice = |scene| {
            parse(
                &[],
                &[
                    ("LASTLIGHT_PRACTICE", "1"),
                    ("LASTLIGHT_REVIEW_SCREEN", scene),
                ],
            )
        };
        assert!(practice("construction").is_ok());
        assert_eq!(
            practice("camp-hmoe").unwrap_err(),
            "Unknown review scene \"camp-hmoe\"; run --list-scenes"
        );
    }
    /// A build without `review` accepts exactly the scenes `--list-scenes`
    /// lists there: the practice scenes.
    #[cfg(not(feature = "review"))]
    #[test]
    fn plain_builds_accept_only_listed_practice_scenes() {
        let practice = |scene| {
            parse(
                &[],
                &[
                    ("LASTLIGHT_PRACTICE", "1"),
                    ("LASTLIGHT_REVIEW_SCREEN", scene),
                ],
            )
        };
        for scene in ["construction", "operations"] {
            assert!(practice(scene).is_ok(), "{scene}");
        }
        for scene in [
            "model-parity-rook",
            "model-workshop",
            "npc-stutter",
            "battle",
        ] {
            assert_eq!(
                practice(scene).unwrap_err(),
                format!("Unknown review scene \"{scene}\"; run --list-scenes"),
            );
        }
    }
    #[cfg(feature = "dev")]
    #[test]
    fn record_and_replay_flags_take_a_path() {
        let c = parse(&["--record", "run.jsonl"], &[]).unwrap();
        assert_eq!(c.record, Some(PathBuf::from("run.jsonl")));
        assert_eq!(c.action, LaunchAction::Play);
        let c = parse(&[], &[("LASTLIGHT_RECORD", "env.jsonl")]).unwrap();
        assert_eq!(c.record, Some(PathBuf::from("env.jsonl")));
        let c = parse(&["--replay", "run.jsonl"], &[]).unwrap();
        assert_eq!(c.action, LaunchAction::Replay);
        assert_eq!(c.replay, Some(PathBuf::from("run.jsonl")));
        assert!(parse(&["--record"], &[]).is_err());
        assert!(parse(&["--replay"], &[]).is_err());
        assert!(parse(&["--replay", "a", "--record", "b"], &[]).is_err());
    }
    #[cfg(not(feature = "dev"))]
    #[test]
    fn builds_without_dev_reject_recording_flags() {
        assert!(parse(&["--record", "a"], &[]).is_err());
        assert!(parse(&["--replay", "a"], &[]).is_err());
        assert_eq!(
            parse(&[], &[("LASTLIGHT_RECORD", "a")])
                .unwrap()
                .warnings
                .len(),
            1
        );
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
    fn unknown_review_scene_is_rejected() {
        let review = |scene| parse(&[], &[("LASTLIGHT_REVIEW_SCREEN", scene)]);
        assert_eq!(
            review("camp-hmoe").unwrap_err(),
            "Unknown review scene \"camp-hmoe\"; run --list-scenes"
        );
        assert!(
            review("model-parity-").is_err(),
            "a workshop prefix needs an id"
        );
        assert!(review("camp").is_err(), "a screen name is not a scene");
        for known in [
            "battle",
            "npc-stutter",
            "gesture-melee",
            "model-parity-rook",
        ] {
            assert!(review(known).is_ok(), "{known}");
        }
    }
    #[cfg(feature = "review")]
    #[test]
    fn review_without_save_dir_never_logs_into_the_player_profile() {
        let c = parse(
            &[],
            &[
                ("LASTLIGHT_REVIEW_SCREEN", "menu"),
                ("LOCALAPPDATA", "C:/x"),
            ],
        )
        .unwrap();
        let player_profile = PathBuf::from("C:/x").join("LastlightHordeDefense");
        assert_eq!(c.save_directory, player_profile);
        assert!(
            !c.log_directory().starts_with(&player_profile),
            "review logs landed in the player profile: {}",
            c.log_directory().display()
        );
        assert_eq!(
            c.log_directory(),
            crate::profile::profile_directory(player_profile, Some("menu"), false).join("logs"),
            "review logs live in the per-process temp review profile"
        );
    }
    #[cfg(feature = "review")]
    #[test]
    fn explicit_save_dir_keeps_review_logs_beside_the_evidence() {
        let c = parse(
            &[],
            &[
                ("LASTLIGHT_REVIEW_SCREEN", "menu"),
                ("LASTLIGHT_SAVE_DIR", "D:/evidence/saves/normal-menu"),
                ("LOCALAPPDATA", "C:/x"),
            ],
        )
        .unwrap();
        assert_eq!(
            c.log_directory(),
            PathBuf::from("D:/evidence/saves/normal-menu").join("logs")
        );
    }
    #[test]
    fn practice_and_ordinary_play_log_into_the_profile() {
        let profile = PathBuf::from("C:/x").join("LastlightHordeDefense");
        let ordinary = parse(&[], &[("LOCALAPPDATA", "C:/x")]).unwrap();
        assert_eq!(ordinary.log_directory(), profile.join("logs"));
        let practice = parse(
            &[],
            &[
                ("LASTLIGHT_PRACTICE", "1"),
                ("LASTLIGHT_REVIEW_SCREEN", "construction"),
                ("LOCALAPPDATA", "C:/x"),
            ],
        )
        .unwrap();
        assert_eq!(practice.log_directory(), profile.join("logs"));
        // A review without a staged scenario plays the real profile, so its
        // logs stay there too.
        let unstaged = parse(
            &[],
            &[("LASTLIGHT_PERF_CPU", "test"), ("LOCALAPPDATA", "C:/x")],
        )
        .unwrap();
        assert_eq!(unstaged.log_directory(), profile.join("logs"));
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
    #[test]
    fn perf_present_mode_and_pipelined_parse_and_are_review_variables() {
        for name in [
            "LASTLIGHT_PERF_PRESENT_MODE",
            "LASTLIGHT_PERF_PIPELINED",
            "LASTLIGHT_PERF_PROFILE",
        ] {
            assert!(REVIEW_VARIABLES.contains(&name), "{name}");
            assert!(HELP.contains(name), "{name}");
        }
        let perf = |vars: &[(&str, &str)]| {
            let mut vars = vars.to_vec();
            vars.push(("LASTLIGHT_PERF_REPORT", "perf.json"));
            parse(&[], &vars).map(|c| c.perf().cloned())
        };
        if !cfg!(feature = "review") {
            // Without review tooling every perf variable is ignored with a warning.
            let c = parse(&[], &[("LASTLIGHT_PERF_PRESENT_MODE", "bogus")]).unwrap();
            assert!(c.review.is_none());
            assert_eq!(c.warnings.len(), 1);
            return;
        }
        let defaults = perf(&[]).unwrap().unwrap();
        assert_eq!((defaults.present_mode, defaults.pipelined), (None, None));
        for (raw, expected) in [
            ("fifo", PresentModeOverride::Fifo),
            ("Mailbox", PresentModeOverride::Mailbox),
            (" immediate ", PresentModeOverride::Immediate),
            ("autonovsync", PresentModeOverride::AutoNoVsync),
        ] {
            let options = perf(&[("LASTLIGHT_PERF_PRESENT_MODE", raw)])
                .unwrap()
                .unwrap();
            assert_eq!(options.present_mode, Some(expected), "{raw}");
        }
        for (raw, expected) in [("0", false), ("1", true)] {
            let options = perf(&[("LASTLIGHT_PERF_PIPELINED", raw)]).unwrap().unwrap();
            assert_eq!(options.pipelined, Some(expected), "{raw}");
        }
        let options = perf(&[("LASTLIGHT_PERF_PROFILE", "playtest")])
            .unwrap()
            .unwrap();
        assert_eq!(options.profile_label.as_deref(), Some("playtest"));
        // Invalid values are typed launch errors.
        assert_eq!(
            perf(&[("LASTLIGHT_PERF_PRESENT_MODE", "vsync")]).unwrap_err(),
            "LASTLIGHT_PERF_PRESENT_MODE must be fifo, mailbox, immediate or autonovsync"
        );
        for raw in ["2", "on", "true"] {
            assert_eq!(
                perf(&[("LASTLIGHT_PERF_PIPELINED", raw)]).unwrap_err(),
                "LASTLIGHT_PERF_PIPELINED must be 0 or 1",
                "{raw}"
            );
        }
        // Each is a review variable on its own: it activates review options.
        let c = parse(&[], &[("LASTLIGHT_PERF_PIPELINED", "0")]).unwrap();
        assert_eq!(c.review().map(|r| r.perf.pipelined), Some(Some(false)));
        assert_eq!(
            PresentModeOverride::AutoNoVsync.present_mode(),
            bevy::window::PresentMode::AutoNoVsync
        );
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
        // An ignored review scene is ordinary play, which logs into the profile.
        let c = parse(
            &[],
            &[
                ("LASTLIGHT_REVIEW_SCREEN", "menu"),
                ("LOCALAPPDATA", "C:/x"),
            ],
        )
        .unwrap();
        assert_eq!(c.log_directory(), c.save_directory.join("logs"));
    }
}
