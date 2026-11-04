//! Enemy sprite atlas mapping, keyed by the enemy catalogue's visual sprite id.
//! `assets/art/enemy_visuals.json` is read and validated once at startup; a new
//! visual variant needs only a data entry for the sprite it names.
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
const BUNDLED: &str = include_str!("../../../../assets/art/enemy_visuals.json");

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    schema_version: u32,
    boss_height: f32,
    default_height: f32,
    sheets: Sheets,
    sprites: Vec<Entry>,
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
        if file.schema_version != 1 {
            return Err(format!(
                "Unsupported enemy visuals schema {}",
                file.schema_version
            ));
        }
        if !in_range(file.boss_height, 0.0, 4.0) || !in_range(file.default_height, 0.0, 3.0) {
            return Err("Enemy visuals boss/default heights are out of range".into());
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
                    let art = SpriteArt::Walk(WalkArt {
                        frames: walk.frames,
                        feet_y: walk.feet_y,
                        height: walk.height,
                    });
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
    // Runs before LogPlugin installs the subscriber, so stderr is the only channel.
    #[allow(clippy::print_stderr)]
    {
        eprintln!(
            "Enemy visuals: {} sprites from {provenance}",
            catalog.sprites.len()
        );
    }
    // A repeated call keeps the first installed mapping, like the core catalogues.
    let _ = ACTIVE.set(catalog);
    Ok(())
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
            ("schema", |v| v["schema_version"] = 2.into()),
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
    fn startup_reads_the_asset_root_and_rejects_a_malformed_file() {
        let root =
            std::env::temp_dir().join(format!("lastlight-enemy-visuals-{}", std::process::id()));
        std::fs::create_dir_all(root.join("art")).unwrap();
        std::fs::write(root.join(FILE), "{ not json").unwrap();
        assert!(initialize(&root).is_err());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
