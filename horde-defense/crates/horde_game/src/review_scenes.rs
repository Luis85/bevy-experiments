//! The review scene registry: every name `LASTLIGHT_REVIEW_SCREEN` accepts.
//! UI-free and compiled in every build, so launch validation and
//! `--list-scenes` never depend on the private review module. Scene names are
//! ids and never change; rows are only appended. The expected screen and tab of
//! every still live in a test table beside the fixtures (`ui/review/tests.rs`).

use SceneKind::{MotionOnly, NeedsInputSave, Workshop};

/// How a scene is captured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneKind {
    /// A still capture_review.ps1 takes by default.
    Still,
    /// Only meaningful as an isolated motion export (capture_motion.ps1).
    MotionOnly,
    /// Needs a copied save generation (`LASTLIGHT_REVIEW_INPUT_SAVE`).
    NeedsInputSave,
    /// Drawn by the review-only player-model workshop.
    Workshop,
}
impl SceneKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Still => "Still",
            Self::MotionOnly => "MotionOnly",
            Self::NeedsInputSave => "NeedsInputSave",
            Self::Workshop => "Workshop",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReviewScene {
    pub name: &'static str,
    pub kind: SceneKind,
    /// capture_motion.ps1 accepts it.
    pub motion: bool,
    /// A performance fixture (measure_performance.ps1, trace_npc.ps1).
    pub perf: bool,
    /// Shipped as a practice scene (`LASTLIGHT_PRACTICE=1`), listed by every build.
    pub practice: bool,
    /// The exact amber primary label from the loop map, once the screen has one.
    pub primary: Option<&'static str>,
    /// The wave-0 scene used as the "before" until this one has its own capture.
    pub before_proxy: Option<&'static str>,
    /// The remit that owns the fixture (read by tests and the capture review).
    #[cfg_attr(not(test), allow(dead_code))]
    pub owner: &'static str,
    pub since_wave: u8,
}

const fn scene(
    name: &'static str,
    kind: SceneKind,
    motion: bool,
    perf: bool,
    practice: bool,
) -> ReviewScene {
    ReviewScene {
        name,
        kind,
        motion,
        perf,
        practice,
        primary: None,
        before_proxy: None,
        owner: "qa-captures",
        since_wave: 0,
    }
}
const fn still(name: &'static str) -> ReviewScene {
    scene(name, SceneKind::Still, false, false, false)
}
const fn motion_still(name: &'static str) -> ReviewScene {
    scene(name, SceneKind::Still, true, false, false)
}
const fn practice_still(name: &'static str) -> ReviewScene {
    scene(name, SceneKind::Still, false, false, true)
}
/// A wave-1 still with its owner and its wave-0 "before" scene.
const fn wave1_still(
    name: &'static str,
    owner: &'static str,
    before_proxy: &'static str,
    motion: bool,
) -> ReviewScene {
    ReviewScene {
        before_proxy: Some(before_proxy),
        owner,
        since_wave: 1,
        ..scene(name, SceneKind::Still, motion, false, false)
    }
}

/// A wave-2 close still (W2-55) with its owner and its wave-0 "before" scene.
const fn wave2_still(
    name: &'static str,
    owner: &'static str,
    before_proxy: &'static str,
    motion: bool,
) -> ReviewScene {
    ReviewScene {
        since_wave: 2,
        ..wave1_still(name, owner, before_proxy, motion)
    }
}

/// A wave-3 UI still (W3-27) with its owner, its wave-0 "before" scene and
/// its amber primary from the loop map, where the screen has one.
const fn wave3_still(
    name: &'static str,
    owner: &'static str,
    before_proxy: &'static str,
    primary: Option<&'static str>,
) -> ReviewScene {
    ReviewScene {
        primary,
        since_wave: 3,
        ..wave1_still(name, owner, before_proxy, false)
    }
}

/// Every named scene. Workshop prefixes are in `WORKSHOP_PREFIXES`.
pub const REVIEW_SCENES: &[ReviewScene] = &[
    // The 41 wave-0 capture_review.ps1 stills, in that script's order.
    motion_still("menu"),
    motion_still("menu-female"),
    motion_still("survivor-select"),
    motion_still("battle-female"),
    still("progressed-battle"),
    still("leveled-armory"),
    still("quality-armory"),
    motion_still("quality-loot"),
    motion_still("reticle-reload"),
    motion_still("damage"),
    motion_still("damage-4x"),
    motion_still("critical-npc"),
    scene("progressed-peak", SceneKind::Still, true, true, false),
    still("world"),
    still("campaign"),
    still("briefing"),
    still("briefing-npc"),
    motion_still("intro"),
    motion_still("battle"),
    motion_still("npc"),
    still("npc-expanded"),
    motion_still("boss"),
    practice_still("construction"),
    still("character"),
    still("character-max"),
    still("survivor"),
    still("armory"),
    still("skills"),
    still("skills-survival"),
    still("skills-ordnance"),
    motion_still("ordnance"),
    still("engineering"),
    still("results"),
    practice_still("operations"),
    still("settings"),
    still("history"),
    still("history-empty"),
    motion_still("arid"),
    motion_still("tropical"),
    scene("peak", SceneKind::Still, true, true, false),
    still("active-camp"),
    // Stills the matrix already staged but the wave-0 script did not list.
    motion_still("max-lanes"),
    still("journey-victory"),
    still("journey-liberated"),
    still("journey-upgrades"),
    still("skills-search"),
    // Not stills.
    scene("gesture-throws", MotionOnly, true, false, false),
    scene("gesture-melee", MotionOnly, true, false, false),
    scene("npc-stutter", NeedsInputSave, false, true, false),
    scene("model-workshop", Workshop, false, false, false),
    scene("model-comparison", Workshop, false, false, false),
    // Wave 1.
    ReviewScene {
        before_proxy: Some("max-lanes"),
        owner: "perf-architecture",
        since_wave: 1,
        // The unsaveable 400-enemy perf fixture; never a practice campaign.
        ..scene("stress", MotionOnly, true, true, false)
    },
    // Wave-1 stills (design-qa-captures 1.8).
    wave1_still("results-defeat", "ux-flow", "results", false),
    wave1_still("save-error", "ux-flow", "world", false),
    wave1_still("night-checkpoint", "qa-captures", "arid", false),
    wave1_still("weapon-classes-pistol", "qa-captures", "battle", true),
    wave1_still("weapon-classes-smg", "qa-captures", "battle", true),
    wave1_still("weapon-classes-shotgun", "qa-captures", "battle", true),
    wave1_still("weapon-classes-rifle", "qa-captures", "battle", true),
    wave1_still("weapon-classes-marksman", "qa-captures", "battle", true),
    wave1_still("weapon-classes-machine-gun", "qa-captures", "battle", true),
    wave1_still("weapon-classes-turret", "qa-captures", "construction", true),
    // The stress fixture as a paused still; never a practice campaign.
    wave1_still("fx-stress", "perf-architecture", "max-lanes", true),
    wave1_still("battle-mid", "ux-flow", "battle", false),
    wave1_still("operations-observer", "ux-flow", "operations", false),
    // Wave-2 close stills (W2-55): W2-37's phase hold and W2-41's Firearms
    // column at x = -1780.
    wave2_still("boss-phase", "vfx-lighting", "boss", true),
    wave2_still("skills-firearms", "progression-tree", "skills", false),
    // W2-57 (CF-2-04): W2-28's ranged windup, a spitter mid-windup at reach.
    wave2_still("spitter-windup", "vfx-lighting", "battle", false),
    // Wave-3 UI stills for goals 2-4 (W3-27, design-qa-captures 1.8).
    wave3_still(
        "upgrades",
        "ux-flow",
        "journey-upgrades",
        Some("Back to battle"),
    ),
    wave3_still(
        "camp-home",
        "ux-flow",
        "active-camp",
        Some("Go to briefing"),
    ),
    // The staged recruit is recruit 1 from Germany, Alex Rowan.
    wave3_still(
        "briefing-recruit",
        "campaign-defenders",
        "briefing-npc",
        Some("Deploy Alex Rowan"),
    ),
    wave3_still("equipment-autosell", "loot-economy", "quality-armory", None),
    wave3_still(
        "results-liberation-recruit",
        "campaign-defenders",
        "journey-liberated",
        Some("Choose next country"),
    ),
    wave3_still("takeover-battle", "campaign-defenders", "npc", None),
];

/// The capture modes of a wave matrix: `capture_review.ps1 -Modes`, and the
/// directory of each still (`reduced` sets `LASTLIGHT_REVIEW_REDUCED`).
#[cfg_attr(not(test), allow(dead_code))]
pub const CAPTURE_MODES: [&str; 2] = ["normal", "reduced"];
/// The capture sizes of a wave matrix as (file suffix, width, height, UI
/// scale): 1440x900@1.0 and the compact 1050x700@1.35.
#[cfg_attr(not(test), allow(dead_code))]
pub const CAPTURE_SIZES: [(&str, u32, u32, f32); 2] =
    [("", 1440, 900, 1.0), ("-small", 1050, 700, 1.35)];

/// One scene of a wave's capture-review matrix.
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MatrixRow {
    /// A registered still.
    pub scene: &'static str,
    /// The rows that changed what the still shows.
    pub changed_by: &'static str,
    /// What the reviewed still must show.
    pub shows: &'static str,
}
#[cfg_attr(not(test), allow(dead_code))]
const fn row(scene: &'static str, changed_by: &'static str, shows: &'static str) -> MatrixRow {
    MatrixRow {
        scene,
        changed_by,
        shows,
    }
}

/// The wave-3 capture matrix (W3-27): the stills the wave-3 review reads,
/// each at both `CAPTURE_SIZES` in both `CAPTURE_MODES` (`expected_stills`).
/// It holds every wave-3 scene with its "before" scene, then the earlier
/// stills wave 3 changed. The closing phase (R36) takes and reviews them:
/// `capture_review.ps1 -Modes normal,reduced` captures every registered
/// still, and this list says which pairs to read and what each must show.
#[cfg_attr(not(test), allow(dead_code))]
pub const WAVE3_MATRIX: &[MatrixRow] = &[
    // The wave-3 scenes.
    row(
        "upgrades",
        "W3-03, W3-27",
        "the Upgrades screen after a first round: the camp row, amber Back to battle and Back \
         to camp above the tree, the recommended node focused",
    ),
    row(
        "camp-home",
        "W3-02, W3-16, W3-25",
        "Camp home: next defense in Poland, You, Mara, a guard and an expedition recruit with \
         status and power, the France outbreak alert, the live expedition, amber Go to briefing",
    ),
    row(
        "briefing-recruit",
        "W3-16",
        "the Poland briefing with the recruit selected in the defender picker and the amber \
         Deploy Alex Rowan",
    ),
    row(
        "equipment-autosell",
        "W3-21",
        "Equipment with the Auto-sell panel open: enabled, Common and below, Would sell 2 now",
    ),
    row(
        "results-liberation-recruit",
        "W3-19, W3-20",
        "Results of the liberating victory: Recruit from Germany with core's price, the loot \
         block with Auto-sold 2, amber Choose next country",
    ),
    row(
        "takeover-battle",
        "W3-16, W3-17",
        "a paused Poland defense held by the recruit: his name on the HUD, his own (male) rig \
         on the field, amber Begin defense, at most two bar rows",
    ),
    // Their "before" scenes.
    row(
        "journey-upgrades",
        "W3-01, W3-16",
        "Results after a first round: the outcome strip and the three-action bar, no tree (the \
         upgrades pair)",
    ),
    row(
        "active-camp",
        "W3-02, W3-18",
        "the world map with a live defense: amber Return to active defense (the camp-home pair)",
    ),
    row(
        "briefing-npc",
        "W3-16, W3-23",
        "the briefing with Mara selected: amber Deploy Mara, the stage preview",
    ),
    row(
        "quality-armory",
        "W3-09, W3-21",
        "Equipment sorted by quality with quality markers; the Auto-sell panel collapsed",
    ),
    row(
        "journey-liberated",
        "W3-19, W3-20, W3-22",
        "Results of a liberation without the Recruitment node: the muted learn-to-recruit line",
    ),
    row(
        "npc",
        "W3-12, W3-16, W3-17",
        "Mara's defense: her name on the HUD, her rig, at most two bar rows",
    ),
    // Earlier stills wave 3 changed.
    row("menu", "W3-01, W3-05", "one amber primary; one Save label"),
    row("survivor-select", "W3-01", "amber Continue as Rook"),
    row(
        "world",
        "W3-01, W3-18",
        "the world map before the first start: amber Start journey",
    ),
    row(
        "campaign",
        "W3-02, W3-18",
        "the 6-entry camp row; Plan defense or Manage country, never a deploy",
    ),
    row(
        "briefing",
        "W3-16, W3-23",
        "amber Deploy personally; the stage preview with its boss badge",
    ),
    row(
        "character",
        "W3-02, W3-22",
        "the camp row with amber Go to briefing; the shared trainable rows",
    ),
    row(
        "survivor",
        "W3-02, W3-22",
        "Mara's training as a drill-down from Camp home",
    ),
    row(
        "armory",
        "W3-21",
        "Equipment with the Manage equipment section",
    ),
    row(
        "operations",
        "W3-02, W3-18",
        "Operations as a drill-down; outbreaks offer Plan response",
    ),
    row(
        "operations-observer",
        "W3-17",
        "the observer window titled with the recruit, drawing the recruit's rig",
    ),
    row(
        "settings",
        "W3-07, W3-08, W3-09, W3-10",
        "Display, quality markers and palette, Music and Interface sliders, the rebind table \
         with the world clock and cycle attack keys",
    ),
    row(
        "results",
        "W3-16, W3-20, W3-22, W3-23",
        "Results with the defender picker, the loot block and the progression lines",
    ),
    row(
        "results-defeat",
        "W3-16, W3-20",
        "a defeat: the cause and tip, amber Try again",
    ),
    row(
        "journey-victory",
        "W3-20, W3-22, W3-23",
        "a first victory: amber Next defense",
    ),
    row("history", "W3-20", "run history with the auto-sold line"),
    row("skills", "W3-22", "the tree with counter names"),
    row(
        "skills-survival",
        "W3-32",
        "no stub or stub label on another drawn wire or node (CF-2-01)",
    ),
    row(
        "save-error",
        "W3-06",
        "the storage modal over the camp: the OS error, amber Retry, Open folder",
    ),
    row(
        "intro",
        "W3-12, W3-29",
        "the first defense paused: amber Begin defense, the pause banner clear of the lanes",
    ),
    row(
        "battle",
        "W3-12, W3-29",
        "at most two bar rows; the pause banner clear of the lanes (CF-1-01)",
    ),
    row(
        "construction",
        "W3-12, W3-29",
        "the construction panel over an unmoved field; the midpoint label left of the panel \
         (CF-1-02)",
    ),
    row(
        "battle-mid",
        "W3-12, W3-29",
        "the 7x16 field: the pause banner clear of the lanes",
    ),
    row(
        "npc-expanded",
        "W3-12, W3-16",
        "the observed defense with its two-row bar",
    ),
    row(
        "boss",
        "W3-12, W3-14",
        "the boss fight with its two-row bar",
    ),
    row(
        "max-lanes",
        "W3-12, W3-29",
        "the 17x30 field: no arena move, the banner clear of the lanes",
    ),
    row(
        "progressed-peak",
        "W3-30, W3-31",
        "wall-hit numbers beside the struck face (CF-1-07); no hover reticle (CF-1-03)",
    ),
    row(
        "fx-stress",
        "W3-31",
        "wall-hit numbers clear of every wall face and stack front (CF-1-07)",
    ),
];

/// The stills a capture run of `matrix` leaves under its output directory,
/// as `capture_review.ps1` names them: `<mode>/<scene>[-small].png`, in
/// matrix order, each scene large then small, normal then reduced.
#[cfg_attr(not(test), allow(dead_code))]
pub fn expected_stills(matrix: &[MatrixRow]) -> Vec<String> {
    matrix
        .iter()
        .flat_map(|row| {
            CAPTURE_MODES.iter().flat_map(move |mode| {
                CAPTURE_SIZES
                    .iter()
                    .map(move |(suffix, ..)| format!("{mode}/{}{suffix}.png", row.scene))
            })
        })
        .collect()
}

/// Workshop scenes named `<prefix><model id>`.
pub const WORKSHOP_PREFIXES: [&str; 4] = [
    "model-parity-",
    "model-parts-",
    "model-matrix-",
    "model-repairs-",
];

/// The kind of a known scene; a workshop prefix plus a non-empty id is Workshop.
pub fn lookup(name: &str) -> Option<SceneKind> {
    REVIEW_SCENES
        .iter()
        .find(|scene| scene.name == name)
        .map(|scene| scene.kind)
        .or_else(|| {
            WORKSHOP_PREFIXES
                .iter()
                .any(|prefix| name.strip_prefix(prefix).is_some_and(|id| !id.is_empty()))
                .then_some(SceneKind::Workshop)
        })
}

pub fn is_workshop(name: &str) -> bool {
    lookup(name) == Some(SceneKind::Workshop)
}

/// Whether a build can stage `name`: exactly the scenes `list_lines` lists for
/// it. Without `review` that is only the practice rows (no workshop names).
pub fn accepts(name: &str, review_build: bool) -> bool {
    if review_build {
        lookup(name).is_some()
    } else {
        REVIEW_SCENES
            .iter()
            .any(|scene| scene.practice && scene.name == name)
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn stills() -> impl Iterator<Item = &'static ReviewScene> {
    REVIEW_SCENES
        .iter()
        .filter(|scene| scene.kind == SceneKind::Still)
}

/// TSV header of `list_lines`.
pub const LIST_HEADER: &str =
    "name\tkind\tmotion\tperf\tpractice\tprimary\tbefore_proxy\tsince_wave";

/// `--list-scenes`: a header, then one TSV row per scene and per workshop
/// prefix (empty fields are absent values). A build without the `review`
/// feature lists only practice scenes, the only ones it can stage.
pub fn list_lines(review_build: bool) -> Vec<String> {
    let rows = REVIEW_SCENES
        .iter()
        .filter(|scene| review_build || scene.practice)
        .map(|scene| {
            format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                scene.name,
                scene.kind.label(),
                scene.motion,
                scene.perf,
                scene.practice,
                scene.primary.unwrap_or(""),
                scene.before_proxy.unwrap_or(""),
                scene.since_wave
            )
        });
    let prefixes = WORKSHOP_PREFIXES
        .iter()
        .filter(|_| review_build)
        .map(|prefix| format!("{prefix}\tWorkshop\tfalse\tfalse\tfalse\t\t\t0"));
    std::iter::once(LIST_HEADER.to_owned())
        .chain(rows)
        .chain(prefixes)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique_and_never_shadow_a_workshop_prefix() {
        let mut names: Vec<_> = REVIEW_SCENES.iter().map(|scene| scene.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), REVIEW_SCENES.len(), "duplicate scene name");
        for scene in REVIEW_SCENES {
            assert!(
                !WORKSHOP_PREFIXES
                    .iter()
                    .any(|prefix| scene.name.starts_with(prefix)),
                "{} hides behind a workshop prefix",
                scene.name
            );
            assert!(!scene.name.contains(['\t', ' ']), "{}", scene.name);
        }
    }
    #[test]
    fn the_wave0_stills_plus_the_five_unlisted_ones_are_46() {
        assert_eq!(stills().filter(|scene| scene.since_wave == 0).count(), 46);
        for name in [
            "max-lanes",
            "journey-victory",
            "journey-liberated",
            "journey-upgrades",
            "skills-search",
        ] {
            assert_eq!(lookup(name), Some(SceneKind::Still), "{name}");
        }
    }
    #[test]
    fn wave1_registers_its_13_stills_with_wave0_before_proxies() {
        let wave1: Vec<_> = stills().filter(|scene| scene.since_wave == 1).collect();
        assert_eq!(wave1.len(), 13);
        for scene in &wave1 {
            let proxy = scene
                .before_proxy
                .expect("a wave-1 still has a before proxy");
            assert!(
                stills().any(|still| still.name == proxy && still.since_wave == 0),
                "{}: before proxy {proxy} is not a wave-0 still",
                scene.name
            );
            assert!(!scene.practice && !scene.perf, "{}", scene.name);
        }
        for name in ["weapon-classes-shotgun", "weapon-classes-machine-gun"] {
            assert!(
                wave1.iter().any(|scene| scene.name == name && scene.motion),
                "capture_motion accepts {name}"
            );
        }
    }
    #[test]
    fn wave3_registers_its_6_stills_with_wave0_before_proxies() {
        let wave3: Vec<_> = stills().filter(|scene| scene.since_wave == 3).collect();
        assert_eq!(
            wave3.iter().map(|scene| scene.name).collect::<Vec<_>>(),
            [
                "upgrades",
                "camp-home",
                "briefing-recruit",
                "equipment-autosell",
                "results-liberation-recruit",
                "takeover-battle",
            ]
        );
        for scene in &wave3 {
            let proxy = scene
                .before_proxy
                .expect("a wave-3 still has a before proxy");
            assert!(
                stills().any(|still| still.name == proxy && still.since_wave == 0),
                "{}: before proxy {proxy} is not a wave-0 still",
                scene.name
            );
            assert!(
                !scene.practice && !scene.perf && !scene.motion,
                "{}",
                scene.name
            );
        }
        assert!(list_lines(true).contains(
            &"camp-home\tStill\tfalse\tfalse\tfalse\tGo to briefing\tactive-camp\t3".into()
        ));
    }
    #[test]
    fn wave3_matrix_rows_are_unique_registered_stills_with_reasons() {
        assert!(!WAVE3_MATRIX.is_empty(), "the W3 matrix is defined");
        let mut names: Vec<_> = WAVE3_MATRIX.iter().map(|row| row.scene).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), WAVE3_MATRIX.len(), "duplicate matrix scene");
        for row in WAVE3_MATRIX {
            assert_eq!(lookup(row.scene), Some(SceneKind::Still), "{}", row.scene);
            assert!(
                row.changed_by.contains("W3-"),
                "{}: a wave-3 row",
                row.scene
            );
            assert!(!row.shows.is_empty(), "{}: what it must show", row.scene);
        }
    }
    #[test]
    fn wave3_matrix_holds_every_wave3_still_and_its_before_scene() {
        let listed = |name: &str| WAVE3_MATRIX.iter().any(|row| row.scene == name);
        let wave3: Vec<_> = stills().filter(|scene| scene.since_wave == 3).collect();
        assert_eq!(wave3.len(), 6);
        for scene in wave3 {
            assert!(listed(scene.name), "{} is not in the matrix", scene.name);
            let proxy = scene.before_proxy.expect("a before proxy");
            assert!(listed(proxy), "{}: its pair {proxy}", scene.name);
        }
        // The stills the wave-3 placement fixes are read on (CF-1-01,
        // CF-1-02, CF-1-03, CF-1-07, CF-2-01) and the W3-06 modal.
        for name in [
            "battle",
            "construction",
            "progressed-peak",
            "fx-stress",
            "skills-survival",
            "save-error",
        ] {
            assert!(listed(name), "{name} is not in the matrix");
        }
    }
    #[test]
    fn wave3_matrix_expects_each_scene_at_both_sizes_in_both_modes() {
        let stills = expected_stills(WAVE3_MATRIX);
        assert_eq!(stills.len(), WAVE3_MATRIX.len() * 4);
        let mut unique = stills.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), stills.len(), "duplicate still path");
        assert_eq!(
            expected_stills(&[row("camp-home", "W3-02", "Camp home")]),
            [
                "normal/camp-home.png",
                "normal/camp-home-small.png",
                "reduced/camp-home.png",
                "reduced/camp-home-small.png",
            ]
        );
        assert_eq!(
            CAPTURE_SIZES,
            [("", 1440, 900, 1.0), ("-small", 1050, 700, 1.35)]
        );
    }
    #[test]
    fn workshop_prefixes_need_an_id() {
        assert_eq!(lookup("model-parity-rook"), Some(SceneKind::Workshop));
        assert!(is_workshop("model-repairs-june") && is_workshop("model-workshop"));
        assert_eq!(lookup("model-parity-"), None);
        assert_eq!(lookup("camp-hmoe"), None);
        assert!(!is_workshop("battle"));
        assert_eq!(lookup("npc-stutter"), Some(SceneKind::NeedsInputSave));
    }
    #[test]
    fn a_build_accepts_exactly_the_scenes_it_lists() {
        for review_build in [true, false] {
            let listed = list_lines(review_build);
            for scene in REVIEW_SCENES {
                let row = listed
                    .iter()
                    .any(|line| line.split('\t').next() == Some(scene.name));
                assert_eq!(accepts(scene.name, review_build), row, "{}", scene.name);
            }
        }
        assert!(accepts("model-parity-rook", true));
        assert!(!accepts("model-parity-rook", false));
        assert!(!accepts("camp-hmoe", true) && !accepts("camp-hmoe", false));
    }
    #[test]
    fn list_lines_are_tsv_and_a_plain_build_lists_only_practice() {
        let review = list_lines(true);
        assert_eq!(review[0], LIST_HEADER);
        let columns = LIST_HEADER.split('\t').count();
        assert!(
            review
                .iter()
                .all(|line| line.split('\t').count() == columns)
        );
        assert_eq!(
            review.len(),
            1 + REVIEW_SCENES.len() + WORKSHOP_PREFIXES.len()
        );
        assert_eq!(
            review
                .iter()
                .filter(|line| line.contains("\tStill\t"))
                .count(),
            46 + 13 + 3 + 6
        );
        assert!(review.contains(&"model-parity-\tWorkshop\tfalse\tfalse\tfalse\t\t\t0".into()));
        assert!(review.contains(&"peak\tStill\ttrue\ttrue\tfalse\t\t\t0".into()));
        let plain = list_lines(false);
        assert_eq!(
            plain,
            vec![
                LIST_HEADER.to_owned(),
                "construction\tStill\tfalse\tfalse\ttrue\t\t\t0".into(),
                "operations\tStill\tfalse\tfalse\ttrue\t\t\t0".into(),
            ]
        );
    }
}
