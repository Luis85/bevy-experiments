//! Enemy sprite atlas mapping, keyed by the enemy catalogue's visual sprite id.
//! `assets/art/enemy_visuals.json` is read and validated once at startup; a new
//! visual variant needs only a data entry for the sprite it names.
//!
//! Schema 2 adds the optional `elite_tells` table (a default tell plus per-affix
//! tells); a schema 1 file still loads and gets [`BUILTIN_ELITE_TELL`].
use horde_core::{
    content::{BossKind, EnemyKind},
    enemy_catalog::{VisualSprite, enemy_catalog},
};
use serde::Deserialize;
use std::{path::Path, sync::OnceLock};

const FILE: &str = "art/enemy_visuals.json";
const MAX_BYTES: u64 = 256 * 1024;
const MAX_SPRITES: usize = 64;
const MAX_FRAMES: usize = 16;
const MAX_AFFIX_TELLS: usize = 16;
/// Survivor art height in source pixels and its figure height in lanes; it
/// takes part in [`EnemyVisualCatalog::crisp_lane_px`] beside the enemy sprites.
const SURVIVOR_ART_PX: f32 = 806.0;
const SURVIVOR_LANES: f32 = 0.87;
const BUNDLED: &str = include_str!("../../../../assets/art/enemy_visuals.json");

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    schema_version: u32,
    boss_height: f32,
    default_height: f32,
    sheets: Sheets,
    sprites: Vec<Entry>,
    /// Schema 2 only; a schema 1 file that names it is rejected.
    #[serde(default)]
    elite_tells: Option<EliteTellsEntry>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EliteTellsEntry {
    default: EliteTell,
    affixes: Vec<AffixTell>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Sheets {
    walkers: [f32; 2],
    actors: [f32; 2],
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    sprite: String,
    height: f32,
    walk: Option<WalkEntry>,
    standing: Option<StandingArt>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WalkEntry {
    feet_y: f32,
    height: f32,
    frames: Vec<WalkFrame>,
}

/// One gait frame on the walker sheet: occupied bounds and the torso pivot.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct WalkFrame {
    pub bounds: [f32; 4],
    pub hip_x: f32,
}
/// A walk cycle sharing one deepest-boot baseline and occupied height.
#[derive(Clone, Debug, PartialEq)]
pub struct WalkArt {
    pub frames: Vec<WalkFrame>,
    pub feet_y: f32,
    pub height: f32,
    /// The widest frame's bounds width, measured once at load: every frame
    /// fits the box at the scale this width allows.
    max_width: f32,
}
impl WalkArt {
    pub fn new(frames: Vec<WalkFrame>, feet_y: f32, height: f32) -> Self {
        let max_width = frames
            .iter()
            .map(|frame| frame.bounds[2] - frame.bounds[0])
            .fold(0., f32::max);
        Self {
            frames,
            feet_y,
            height,
            max_width,
        }
    }
    pub fn max_width(&self) -> f32 {
        self.max_width
    }
}
/// A single standing pose on the actor sheet: bounds, torso, deepest boot, height.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StandingArt {
    pub bounds: [f32; 4],
    pub hip_x: f32,
    pub feet_y: f32,
    pub height: f32,
}
/// The shape drawn above an elite's health bar; shape, not colour, carries the
/// meaning so the tell stays colour-blind safe.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))]
pub enum TellMarker {
    Chevron,
    Crown,
    Pips1,
    Pips2,
    Pips3,
    Shield,
    Spark,
}
/// How an elite reads on the field: outline tint, ground ring width, glow
/// strength and pulse rate (sim-tick based, so it freezes on pause).
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
#[cfg_attr(not(test), allow(dead_code))]
pub struct EliteTell {
    pub outline_rgb: [u8; 3],
    pub marker: TellMarker,
    /// Ground ring width in pixels, 1..=4.
    pub ring_px: u8,
    /// Tint lerp strength toward `outline_rgb`, 0..=0.4.
    pub glow: f32,
    /// Pulse frequency in Hz, 0..=2.0.
    pub pulse_hz: f32,
}
/// A tell for one elite affix id.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
#[cfg_attr(not(test), allow(dead_code))]
pub struct AffixTell {
    pub affix: String,
    pub tell: EliteTell,
}
/// The tell a schema 1 file, or a schema 2 file without `elite_tells`, gets:
/// an AMBER ring with a chevron, glow 0.15 at 1.2 Hz.
pub const BUILTIN_ELITE_TELL: EliteTell = EliteTell {
    outline_rgb: [239, 181, 82],
    marker: TellMarker::Chevron,
    ring_px: 2,
    glow: 0.15,
    pulse_hz: 1.2,
};
#[derive(Clone, Debug, PartialEq)]
pub enum SpriteArt {
    Walk(WalkArt),
    Standing(StandingArt),
}
/// Resolved art for one catalogue sprite.
#[derive(Debug, PartialEq)]
pub struct EnemySprite {
    pub sprite: VisualSprite,
    /// Stable position in the catalogue; indexes per-sprite caches such as flash masks.
    pub index: usize,
    /// Figure height in lanes before the archetype's scale.
    pub height: f32,
    /// Pixel size of the sheet the art is measured on.
    pub source_size: [f32; 2],
    pub art: SpriteArt,
}
impl EnemySprite {
    pub fn is_walk(&self) -> bool {
        matches!(self.art, SpriteArt::Walk(_))
    }
}
#[derive(Debug)]
pub struct EnemyVisualCatalog {
    /// Boss figure height in lanes; bosses ignore archetype scale.
    pub boss_height: f32,
    /// Height of a legacy body that names no kind or visual.
    pub default_height: f32,
    sprites: Vec<EnemySprite>,
    elite_default: EliteTell,
    elite_affixes: Vec<AffixTell>,
}
impl EnemyVisualCatalog {
    pub fn from_json(text: &str) -> Result<Self, String> {
        let file: File =
            serde_json::from_str(text).map_err(|e| format!("Invalid enemy visuals: {e}"))?;
        let catalog = Self::compile(file)?;
        catalog.validate_coverage()?;
        Ok(catalog)
    }
    fn compile(file: File) -> Result<Self, String> {
        let (elite_default, elite_affixes) = match (file.schema_version, file.elite_tells) {
            (1, None) | (2, None) => (BUILTIN_ELITE_TELL, Vec::new()),
            (1, Some(_)) => {
                return Err("Enemy visuals schema 1 cannot carry elite_tells".into());
            }
            (2, Some(tells)) => {
                validate_tell("default", &tells.default)?;
                if tells.affixes.len() > MAX_AFFIX_TELLS {
                    return Err(format!(
                        "Enemy visuals allow at most {MAX_AFFIX_TELLS} affix tells"
                    ));
                }
                for (index, entry) in tells.affixes.iter().enumerate() {
                    if entry.affix.trim().is_empty() {
                        return Err("Enemy visuals affix tell needs an affix id".into());
                    }
                    if tells.affixes[..index]
                        .iter()
                        .any(|known| known.affix == entry.affix)
                    {
                        return Err(format!("Enemy visuals repeat affix tell {}", entry.affix));
                    }
                    validate_tell(&entry.affix, &entry.tell)?;
                }
                (tells.default, tells.affixes)
            }
            (other, _) => {
                return Err(format!("Unsupported enemy visuals schema {other}"));
            }
        };
        if !in_range(file.boss_height, 0.0, 4.0) || !in_range(file.default_height, 0.0, 3.0) {
            return Err("Enemy visuals boss/default heights are out of range".into());
        }
        // Survivors share the actor sheet; one size for both, checked here.
        if file.sheets.actors != super::ACTOR_SIZE {
            return Err(format!(
                "Enemy visuals actor sheet {:?} differs from the survivor atlas {:?}",
                file.sheets.actors,
                super::ACTOR_SIZE
            ));
        }
        for size in [file.sheets.walkers, file.sheets.actors] {
            if size.iter().any(|v| !in_range(*v, 0.0, 16384.0)) {
                return Err("Enemy visuals sheet sizes must be positive pixels".into());
            }
        }
        if file.sprites.is_empty() || file.sprites.len() > MAX_SPRITES {
            return Err(format!("Enemy visuals need 1..={MAX_SPRITES} sprites"));
        }
        let mut sprites: Vec<EnemySprite> = Vec::with_capacity(file.sprites.len());
        for (index, entry) in file.sprites.into_iter().enumerate() {
            let name = entry.sprite;
            let sprite: VisualSprite =
                serde_json::from_value(serde_json::Value::String(name.clone()))
                    .map_err(|_| format!("Enemy visuals name unknown sprite {name}"))?;
            if sprites.iter().any(|known| known.sprite == sprite) {
                return Err(format!("Enemy visuals repeat sprite {name}"));
            }
            if !in_range(entry.height, 0.0, 3.0) {
                return Err(format!("Enemy visual {name} height must be in (0, 3]"));
            }
            let (art, source_size) = match (entry.walk, entry.standing) {
                (Some(walk), None) => {
                    let size = file.sheets.walkers;
                    if walk.frames.is_empty() || walk.frames.len() > MAX_FRAMES {
                        return Err(format!("Enemy visual {name} needs 1..={MAX_FRAMES} frames"));
                    }
                    if !in_range(walk.height, 0.0, size[1]) {
                        return Err(format!("Enemy visual {name} walk height is invalid"));
                    }
                    for frame in &walk.frames {
                        anchored(&name, frame.bounds, frame.hip_x, walk.feet_y, size)?;
                    }
                    let art = SpriteArt::Walk(WalkArt::new(walk.frames, walk.feet_y, walk.height));
                    (art, size)
                }
                (None, Some(standing)) => {
                    let size = file.sheets.actors;
                    if !in_range(standing.height, 0.0, size[1]) {
                        return Err(format!("Enemy visual {name} standing height is invalid"));
                    }
                    anchored(
                        &name,
                        standing.bounds,
                        standing.hip_x,
                        standing.feet_y,
                        size,
                    )?;
                    (SpriteArt::Standing(standing), size)
                }
                _ => {
                    return Err(format!(
                        "Enemy visual {name} needs exactly one of walk or standing"
                    ));
                }
            };
            sprites.push(EnemySprite {
                sprite,
                index,
                height: entry.height,
                source_size,
                art,
            });
        }
        Ok(Self {
            boss_height: file.boss_height,
            default_height: file.default_height,
            sprites,
            elite_default,
            elite_affixes,
        })
    }
    /// Every sprite the enemy catalogue or a legacy save can name has art.
    fn validate_coverage(&self) -> Result<(), String> {
        let catalog = enemy_catalog();
        let mut required: Vec<(String, VisualSprite)> = catalog
            .archetype_ids()
            .map(|id| {
                catalog
                    .resolve(id)
                    .map(|enemy| (format!("archetype {id}"), enemy.visual.sprite))
            })
            .collect::<Result<_, _>>()?;
        for kind in EnemyKind::ALL {
            required.push((format!("{kind:?}"), legacy_sprite(kind, None)));
        }
        for boss in [BossKind::Breacher, BossKind::Caller, BossKind::SiegeBrute] {
            required.push((
                format!("{boss:?}"),
                legacy_sprite(EnemyKind::Brute, Some(boss)),
            ));
        }
        for (owner, sprite) in required {
            if self.get(sprite).is_none() {
                return Err(format!("Enemy visuals lack {sprite:?} used by {owner}"));
            }
        }
        Ok(())
    }
    pub fn get(&self, sprite: VisualSprite) -> Option<&EnemySprite> {
        self.sprites.iter().find(|known| known.sprite == sprite)
    }
    pub fn sprites(&self) -> &[EnemySprite] {
        &self.sprites
    }
    /// The tell for an elite with `affix`; no affix or an unknown one gets the default.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn elite_tell(&self, affix: Option<&str>) -> &EliteTell {
        affix
            .and_then(|id| self.elite_affixes.iter().find(|known| known.affix == id))
            .map_or(&self.elite_default, |known| &known.tell)
    }
    /// Source pixels per lane at which the coarsest art draws 1:1: the minimum
    /// over sprites of art height / figure height in lanes (boss art uses the
    /// larger of the boss height and its own pack height; archetype scale is
    /// ignored), and the survivor's 806 px / 0.87.
    /// Sheets sample NEAREST; the uniform zoom-to-fit camera does not clamp
    /// against this (user camera ruling, `art::camera::fit_field`), so it is a
    /// measurement for the sheet-upscale human check only.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn crisp_lane_px(&self) -> f32 {
        let bosses = [BossKind::Breacher, BossKind::Caller, BossKind::SiegeBrute]
            .map(|boss| legacy_sprite(EnemyKind::Brute, Some(boss)));
        self.sprites
            .iter()
            .map(|sprite| {
                let art_px = match &sprite.art {
                    SpriteArt::Walk(walk) => walk.height,
                    SpriteArt::Standing(standing) => standing.height,
                };
                // Boss art also draws as pack enemies at the sprite height.
                let lanes = if bosses.contains(&sprite.sprite) {
                    self.boss_height.max(sprite.height)
                } else {
                    sprite.height
                };
                art_px / lanes
            })
            .fold(SURVIVOR_ART_PX / SURVIVOR_LANES, f32::min)
    }
}

/// [`EnemyVisualCatalog::crisp_lane_px`] of the active mapping.
#[cfg_attr(not(test), allow(dead_code))]
pub fn crisp_lane_px() -> f32 {
    active().crisp_lane_px()
}

fn validate_tell(owner: &str, tell: &EliteTell) -> Result<(), String> {
    let within = |value: f32, high: f32| value.is_finite() && (0.0..=high).contains(&value);
    if !(1..=4).contains(&tell.ring_px) || !within(tell.glow, 0.4) || !within(tell.pulse_hz, 2.0) {
        return Err(format!(
            "Enemy visuals elite tell {owner} needs ring_px 1..=4, glow 0..=0.4, pulse_hz 0..=2"
        ));
    }
    Ok(())
}

/// The sprite an enemy saved without a resolved profile draws as.
pub fn legacy_sprite(kind: EnemyKind, boss: Option<BossKind>) -> VisualSprite {
    match boss {
        Some(BossKind::Breacher) => VisualSprite::Breacher,
        Some(BossKind::Caller) => VisualSprite::Caller,
        Some(BossKind::SiegeBrute) => VisualSprite::SiegeBrute,
        None => match kind {
            EnemyKind::Shambler => VisualSprite::Shambler,
            EnemyKind::Runner => VisualSprite::Runner,
            EnemyKind::Brute => VisualSprite::Brute,
            EnemyKind::Armored => VisualSprite::Armored,
            EnemyKind::Demolisher => VisualSprite::Demolisher,
        },
    }
}

fn in_range(value: f32, low: f32, high: f32) -> bool {
    value.is_finite() && value > low && value <= high
}
/// Bounds fit the sheet; the torso pivot and boot baseline lie inside them.
fn anchored(
    name: &str,
    bounds: [f32; 4],
    hip_x: f32,
    feet_y: f32,
    size: [f32; 2],
) -> Result<(), String> {
    let [left, top, right, bottom] = bounds;
    let fits = bounds.iter().all(|v| v.is_finite())
        && left >= 0.0
        && top >= 0.0
        && right <= size[0]
        && bottom <= size[1]
        && left < right
        && top < bottom;
    if !fits {
        return Err(format!("Enemy visual {name} bounds leave the sheet"));
    }
    if !(left < hip_x && hip_x < right && top < feet_y && feet_y <= bottom) {
        return Err(format!(
            "Enemy visual {name} anchor lies outside its bounds"
        ));
    }
    Ok(())
}

static ACTIVE: OnceLock<EnemyVisualCatalog> = OnceLock::new();

/// Startup only, after the core catalogues: reads and validates the asset
/// root's mapping. A missing file uses the bundled copy; a bad one is an error.
pub fn initialize(asset_root: &Path) -> Result<(), String> {
    let (catalog, provenance) = load(asset_root)?;
    // Runs before LogPlugin installs the subscriber, so stderr is the only channel.
    #[allow(clippy::print_stderr)]
    {
        eprintln!(
            "Enemy visuals: {} sprites from {provenance}",
            catalog.sprites.len()
        );
    }
    ACTIVE
        .set(catalog)
        .map_err(|_| "Enemy visuals were already installed or read before startup".into())
}

/// Reads and validates the asset root's mapping, with its provenance.
fn load(asset_root: &Path) -> Result<(EnemyVisualCatalog, String), String> {
    let path = asset_root.join(FILE);
    let (text, provenance) = match std::fs::metadata(&path) {
        Ok(meta) if meta.len() > MAX_BYTES => {
            return Err(format!("{} exceeds {MAX_BYTES} bytes", path.display()));
        }
        Ok(_) => (
            std::fs::read_to_string(&path)
                .map_err(|e| format!("Cannot read {}: {e}", path.display()))?,
            path.display().to_string(),
        ),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            (BUNDLED.to_owned(), "bundled fallback (file absent)".into())
        }
        Err(e) => return Err(format!("Cannot inspect {}: {e}", path.display())),
    };
    let catalog =
        EnemyVisualCatalog::from_json(&text).map_err(|e| format!("{e} ({provenance})"))?;
    Ok((catalog, provenance))
}

/// The startup mapping, or the bundled one where startup did not run (tests).
pub fn active() -> &'static EnemyVisualCatalog {
    ACTIVE.get_or_init(|| {
        EnemyVisualCatalog::from_json(BUNDLED).expect("bundled enemy visuals validate")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bundled() -> serde_json::Value {
        serde_json::from_str(BUNDLED).unwrap()
    }
    fn compile(value: serde_json::Value) -> Result<EnemyVisualCatalog, String> {
        EnemyVisualCatalog::from_json(&value.to_string())
    }

    #[test]
    fn bundled_mapping_covers_every_catalogue_archetype_and_legacy_kind() {
        let catalog = EnemyVisualCatalog::from_json(BUNDLED).unwrap();
        for id in enemy_catalog().archetype_ids() {
            let sprite = enemy_catalog().resolve(id).unwrap().visual.sprite;
            assert!(catalog.get(sprite).is_some(), "{id}");
        }
        for (index, sprite) in catalog.sprites().iter().enumerate() {
            assert_eq!(sprite.index, index);
        }
        // Heights are the same f32 values the hard-coded mapping used.
        assert_eq!(catalog.boss_height, 1.08_f32);
        assert_eq!(catalog.default_height, 0.78_f32);
        assert_eq!(catalog.get(VisualSprite::Brute).unwrap().height, 0.9_f32);
        assert_eq!(
            catalog.get(VisualSprite::Shambler).unwrap().height,
            0.78_f32
        );
        assert!(catalog.get(VisualSprite::Runner).unwrap().is_walk());
        assert!(!catalog.get(VisualSprite::Caller).unwrap().is_walk());
    }

    /// W2-31 (design enemy-balance §4 "Records"): the 7 appended archetypes
    /// need no new art. Each draws a walk sprite one of the 11 earlier
    /// archetypes already draws, with the design table's tint and scale.
    #[test]
    fn every_new_archetype_uses_an_existing_visual_sprite() {
        const EARLIER: [&str; 11] = [
            "shambler",
            "runner",
            "brute",
            "armored",
            "demolisher",
            "breacher-pack",
            "caller-pack",
            "siege-pack",
            "skitter",
            "plated-runner",
            "hungry-shambler",
        ];
        let new: [(&str, VisualSprite, [f32; 3], f32); 7] = [
            ("exploder", VisualSprite::Shambler, [0.72, 0.9, 0.5], 1.15),
            ("splitter", VisualSprite::Brute, [0.85, 0.78, 0.9], 1.05),
            (
                "splitter-brood",
                VisualSprite::Runner,
                [0.85, 0.78, 0.9],
                0.6,
            ),
            ("spitter", VisualSprite::Runner, [0.6, 0.9, 0.85], 0.95),
            (
                "shield-bearer",
                VisualSprite::Armored,
                [0.8, 0.85, 0.95],
                1.05,
            ),
            ("sprinter", VisualSprite::Runner, [1.0, 0.85, 0.7], 0.85),
            ("juggernaut", VisualSprite::Brute, [0.7, 0.7, 0.75], 1.4),
        ];
        let art = EnemyVisualCatalog::from_json(BUNDLED).unwrap();
        let existing: Vec<VisualSprite> = EARLIER
            .iter()
            .map(|id| enemy_catalog().resolve(id).unwrap().visual.sprite)
            .collect();
        for (id, sprite, [r, g, b], scale) in new {
            let visual = enemy_catalog().resolve(id).unwrap().visual;
            assert_eq!(visual.sprite, sprite, "{id}");
            assert!(
                existing.contains(&sprite),
                "{id}: {sprite:?} is existing art"
            );
            assert!(art.get(sprite).is_some_and(EnemySprite::is_walk), "{id}");
            assert_eq!(visual.tint, [r, g, b, 1.0], "{id}");
            assert_eq!(visual.scale, scale, "{id}");
        }
        assert_eq!(
            enemy_catalog().archetype_ids().count(),
            EARLIER.len() + new.len()
        );
    }

    #[test]
    fn v2_with_default_elite_tell_parses() {
        let mut value = bundled();
        value["schema_version"] = 2.into();
        // A default unlike the built-in one, so the file's own default is seen.
        value["elite_tells"] = serde_json::json!({
            "default": {
                "outline_rgb": [86, 180, 233], "marker": "Shield",
                "ring_px": 1, "glow": 0.3, "pulse_hz": 0.5
            },
            "affixes": [{
                "affix": "volatile",
                "tell": {
                    "outline_rgb": [213, 94, 0], "marker": "Spark",
                    "ring_px": 3, "glow": 0.4, "pulse_hz": 2.0
                }
            }]
        });
        let catalog = compile(value).unwrap();
        let file_default = EliteTell {
            outline_rgb: [86, 180, 233],
            marker: TellMarker::Shield,
            ring_px: 1,
            glow: 0.3,
            pulse_hz: 0.5,
        };
        assert_ne!(file_default, BUILTIN_ELITE_TELL);
        assert_eq!(catalog.elite_tell(None), &file_default);
        assert_eq!(catalog.elite_tell(Some("unknown")), &file_default);
        let volatile = catalog.elite_tell(Some("volatile"));
        assert_eq!(volatile.marker, TellMarker::Spark);
        assert_eq!(volatile.outline_rgb, [213, 94, 0]);
        assert_eq!(volatile.ring_px, 3);
        // The bundled file is schema 2 with the default tell only.
        assert_eq!(bundled()["schema_version"], 2);
        let shipped = active();
        assert_eq!(shipped.elite_tell(None), &BUILTIN_ELITE_TELL);
        assert!(shipped.elite_affixes.is_empty());
        // Schema 2 without the table also gets the built-in tell.
        let mut bare = bundled();
        bare.as_object_mut().unwrap().remove("elite_tells");
        assert_eq!(compile(bare).unwrap().elite_tell(None), &BUILTIN_ELITE_TELL);
    }

    #[test]
    fn v1_fixture_loads_and_gets_builtin_elite_tell() {
        let text = include_str!("fixtures/enemy_visuals_v1.json");
        let raw: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(raw["schema_version"], 1);
        assert!(raw.get("elite_tells").is_none());
        let v1 = EnemyVisualCatalog::from_json(text).unwrap();
        assert_eq!(v1.elite_tell(None), &BUILTIN_ELITE_TELL);
        assert_eq!(v1.elite_tell(Some("volatile")), &BUILTIN_ELITE_TELL);
        assert_eq!(BUILTIN_ELITE_TELL.marker, TellMarker::Chevron);
        assert_eq!(BUILTIN_ELITE_TELL.outline_rgb, [239, 181, 82]);
        // Same art as the bundled v2 file: v2 changed only the tells.
        let v2 = EnemyVisualCatalog::from_json(BUNDLED).unwrap();
        assert_eq!(v1.sprites(), v2.sprites());
        assert_eq!(v1.boss_height, v2.boss_height);
        assert_eq!(v1.default_height, v2.default_height);
    }

    #[test]
    fn schema_3_and_v1_with_elite_tells_are_rejected() {
        let mut v3 = bundled();
        v3["schema_version"] = 3.into();
        assert_eq!(
            compile(v3).unwrap_err(),
            "Unsupported enemy visuals schema 3"
        );
        let mut v1 = bundled();
        v1["schema_version"] = 1.into();
        assert!(v1.get("elite_tells").is_some());
        assert!(compile(v1.clone()).unwrap_err().contains("schema 1"));
        v1.as_object_mut().unwrap().remove("elite_tells");
        assert!(compile(v1).is_ok());
    }

    #[test]
    fn elite_tell_bounds_rejected() {
        type Edit = fn(&mut serde_json::Value);
        let tell = serde_json::json!({
            "outline_rgb": [86, 180, 233], "marker": "Shield",
            "ring_px": 1, "glow": 0.0, "pulse_hz": 0.0
        });
        let affix = |id: &str| serde_json::json!({ "affix": id, "tell": tell.clone() });
        // The bounds themselves are accepted.
        let mut edge = bundled();
        edge["elite_tells"]["default"]["ring_px"] = 4.into();
        edge["elite_tells"]["affixes"] = (0..16).map(|i| affix(&format!("a{i}"))).collect();
        let edge = compile(edge).unwrap();
        assert_eq!(edge.elite_tell(None).ring_px, 4);
        assert_eq!(edge.elite_tell(Some("a15")).marker, TellMarker::Shield);
        let cases: [(&str, Edit); 14] = [
            ("ring 0", |v| {
                v["elite_tells"]["default"]["ring_px"] = 0.into()
            }),
            ("ring 5", |v| {
                v["elite_tells"]["default"]["ring_px"] = 5.into()
            }),
            ("glow", |v| {
                v["elite_tells"]["default"]["glow"] = 0.41.into()
            }),
            ("negative glow", |v| {
                v["elite_tells"]["default"]["glow"] = (-0.01).into()
            }),
            ("pulse", |v| {
                v["elite_tells"]["default"]["pulse_hz"] = 2.01.into()
            }),
            ("negative pulse", |v| {
                v["elite_tells"]["default"]["pulse_hz"] = (-0.5).into()
            }),
            ("unknown marker", |v| {
                v["elite_tells"]["default"]["marker"] = "Skull".into()
            }),
            ("unknown tell field", |v| {
                v["elite_tells"]["default"]["blink"] = 1.into()
            }),
            ("affix out of range", |v| {
                let mut bad = v["elite_tells"]["default"].clone();
                bad["pulse_hz"] = 3.into();
                v["elite_tells"]["affixes"] = serde_json::json!([{ "affix": "x", "tell": bad }]);
            }),
            ("blank affix id", |v| {
                let tell = v["elite_tells"]["default"].clone();
                v["elite_tells"]["affixes"] = serde_json::json!([{ "affix": " ", "tell": tell }]);
            }),
            ("unknown elite_tells field", |v| {
                v["elite_tells"]["fallback"] = v["elite_tells"]["default"].clone()
            }),
            ("unknown affix tell field", |v| {
                let tell = v["elite_tells"]["default"].clone();
                v["elite_tells"]["affixes"] =
                    serde_json::json!([{ "affix": "x", "tell": tell, "note": "" }]);
            }),
            ("duplicate affix", |v| {
                let tell = v["elite_tells"]["default"].clone();
                let entry = serde_json::json!({ "affix": "x", "tell": tell });
                v["elite_tells"]["affixes"] = serde_json::json!([entry.clone(), entry]);
            }),
            ("17 affixes", |v| {
                let tell = v["elite_tells"]["default"].clone();
                v["elite_tells"]["affixes"] = (0..17)
                    .map(|i| serde_json::json!({ "affix": format!("a{i}"), "tell": tell }))
                    .collect();
            }),
        ];
        for (name, edit) in cases {
            let mut value = bundled();
            edit(&mut value);
            assert!(compile(value).is_err(), "{name} must be rejected");
        }
    }

    #[test]
    fn crisp_lane_px_is_measured_from_sprite_art() {
        // Brute 267 px / 0.90 lanes is the coarsest art today.
        let crisp = crisp_lane_px();
        assert!((crisp - 296.7).abs() <= 0.5, "{crisp}");
        assert_eq!(crisp, 267.0_f32 / 0.9_f32);
        // Bosses measure against boss_height and the survivor bounds it above.
        let mut value = bundled();
        value["boss_height"] = 2.0.into();
        assert_eq!(compile(value).unwrap().crisp_lane_px(), 416.0_f32 / 2.0);
        let mut value = bundled();
        for sprite in value["sprites"].as_array_mut().unwrap() {
            sprite["height"] = 0.1.into();
        }
        value["boss_height"] = 0.1.into();
        assert_eq!(compile(value).unwrap().crisp_lane_px(), 806.0_f32 / 0.87);
        // Boss art also draws as pack enemies (breacher-pack) at the sprite's
        // own height, so a boss height below it does not hide that size.
        let mut value = bundled();
        let breacher = value["sprites"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|sprite| sprite["sprite"] == "Breacher")
            .unwrap();
        breacher["height"] = 2.0.into();
        value["boss_height"] = 0.5.into();
        assert_eq!(compile(value).unwrap().crisp_lane_px(), 419.0_f32 / 2.0);
    }

    #[test]
    fn a_new_variant_is_data_only_and_frame_counts_follow_the_data() {
        // Re-pointing a sprite at different cells needs no code: a two-frame
        // shambler cycle and a standing runner both compile.
        let mut value = bundled();
        let sprites = value["sprites"].as_array_mut().unwrap();
        sprites[0]["walk"]["frames"]
            .as_array_mut()
            .unwrap()
            .truncate(2);
        sprites[1]["standing"] = serde_json::json!({
            "bounds": [917, 10, 1293, 435], "hip_x": 1109, "feet_y": 432, "height": 419
        });
        sprites[1].as_object_mut().unwrap().remove("walk");
        let catalog = compile(value).unwrap();
        let SpriteArt::Walk(walk) = &catalog.get(VisualSprite::Shambler).unwrap().art else {
            panic!("walk art");
        };
        assert_eq!(walk.frames.len(), 2);
        assert!(!catalog.get(VisualSprite::Runner).unwrap().is_walk());
    }

    #[test]
    fn invalid_mappings_fail_with_a_clean_error() {
        type Edit = fn(&mut serde_json::Value);
        let cases: [(&str, Edit); 8] = [
            ("schema", |v| v["schema_version"] = 0.into()),
            ("missing sprite", |v| {
                v["sprites"].as_array_mut().unwrap().remove(2);
            }),
            ("unknown sprite", |v| {
                v["sprites"][0]["sprite"] = "Shamblr".into()
            }),
            ("duplicate", |v| {
                v["sprites"][1]["sprite"] = "Shambler".into()
            }),
            ("off sheet", |v| {
                v["sprites"][0]["walk"]["frames"][0]["bounds"][2] = 5000.into();
            }),
            ("anchor outside", |v| {
                v["sprites"][5]["standing"]["hip_x"] = 10.into();
            }),
            ("both arts", |v| {
                v["sprites"][5]["walk"] = v["sprites"][0]["walk"].clone();
            }),
            ("unknown field", |v| v["sprites"][0]["tint"] = 1.into()),
        ];
        for (name, edit) in cases {
            let mut value = bundled();
            edit(&mut value);
            assert!(compile(value).is_err(), "{name} must be rejected");
        }
    }

    #[test]
    fn startup_reads_the_asset_root_falls_back_when_absent_and_rejects_bad_files() {
        let root =
            std::env::temp_dir().join(format!("lastlight-enemy-visuals-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("art")).unwrap();
        // Absent: the bundled mapping.
        let (bundled, provenance) = load(&root).unwrap();
        assert!(provenance.contains("bundled"));
        assert_eq!(bundled.sprites().len(), active().sprites().len());
        // Present and valid: read from the asset root (a changed boss height proves it).
        let mut value = serde_json::from_str::<serde_json::Value>(BUNDLED).unwrap();
        value["boss_height"] = 1.5.into();
        std::fs::write(root.join(FILE), value.to_string()).unwrap();
        let (read, provenance) = load(&root).unwrap();
        assert_eq!(read.boss_height, 1.5);
        assert!(provenance.contains("enemy_visuals.json"));
        // Malformed, or naming a different survivor sheet size: clean errors.
        std::fs::write(root.join(FILE), "{ not json").unwrap();
        assert!(load(&root).is_err());
        value["boss_height"] = 1.08.into();
        value["sheets"]["actors"] = serde_json::json!([3548, 1774]);
        std::fs::write(root.join(FILE), value.to_string()).unwrap();
        assert!(load(&root).unwrap_err().contains("survivor atlas"));
        std::fs::remove_dir_all(&root).unwrap();
    }
}
