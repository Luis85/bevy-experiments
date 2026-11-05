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
        assert_eq!(stills().count(), 46);
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
    fn workshop_prefixes_need_an_id() {
        assert_eq!(lookup("model-parity-rook"), Some(SceneKind::Workshop));
        assert!(is_workshop("model-repairs-june") && is_workshop("model-workshop"));
        assert_eq!(lookup("model-parity-"), None);
        assert_eq!(lookup("camp-hmoe"), None);
        assert!(!is_workshop("battle"));
        assert_eq!(lookup("npc-stutter"), Some(SceneKind::NeedsInputSave));
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
            46
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
