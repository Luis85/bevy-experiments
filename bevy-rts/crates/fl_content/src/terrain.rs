//! Terrain types: `*.terrain.ron` and the movement classes they block (ADR-0014).

use fl_math::Fx;
use serde::{Deserialize, Serialize};

use crate::kinds::ContentKind;
use crate::link::LinkCtx;

/// The most terrain kinds content may load: a cell's type is a `u8`, and `u8::MAX` means
/// "no terrain kind" (open ground) in the sim's terrain grid.
pub(crate) const MAX_TERRAIN_KINDS: usize = u8::MAX as usize;

/// How a unit moves over terrain. Navigation data is shared per class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum MoveClass {
    Foot,
    Wheeled,
    HalfTrack,
    Tracked,
}

impl MoveClass {
    /// Every class, in declaration order.
    pub const ALL: [MoveClass; 4] = [
        MoveClass::Foot,
        MoveClass::Wheeled,
        MoveClass::HalfTrack,
        MoveClass::Tracked,
    ];

    /// The class's position in [`ALL`](Self::ALL), `0..4`: an index into per-class tables.
    pub fn index(self) -> usize {
        self as usize
    }
}

/// A terrain type: `*.terrain.ron`. Map cells name one through their map's legend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerrainDef {
    pub id: String,
    pub name: String,
    /// Passability in [0, 1]: the fraction of road speed every unit keeps here (spec §7).
    pub pass: Fx,
    /// Movement classes that cannot enter cells of this type; no duplicates.
    pub blocks: Vec<MoveClass>,
    /// Linear RGB, 0–1. Presentation only, but rules-hashed with the rest of the item.
    pub color: (f32, f32, f32),
}

impl ContentKind for TerrainDef {
    const EXT: &'static str = "terrain";
    const IN_RULES_HASH: bool = true;

    fn id(&self) -> &str {
        &self.id
    }

    fn link(&self, ctx: &mut LinkCtx<'_>) {
        if self.pass < Fx::ZERO || self.pass > Fx::ONE {
            ctx.error("pass", format!("{} must lie in [0, 1]", self.pass));
        }
        for (i, class) in self.blocks.iter().enumerate() {
            if self.blocks[..i].contains(class) {
                ctx.error(
                    "blocks",
                    format!("{class:?} is listed more than once (blocks[{i}])"),
                );
            }
        }
    }
}
