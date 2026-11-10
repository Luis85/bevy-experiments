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
];

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
            46 + 13 + 2
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
