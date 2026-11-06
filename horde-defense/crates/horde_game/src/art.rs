mod backdrop;
mod battlefield;
pub mod camera;
mod figures;
mod palette;
mod projection;
mod scenes;
#[cfg(test)]
mod tests;

use backdrop::{box_at, line, paint_ground};
pub use backdrop::{world_atmosphere, world_backdrop};
pub use battlefield::{RenderOptions, battlefield, battlefield_for_attempt, sample_motion};
pub use camera::{FieldFit, clearing_size, fit_field};
pub use figures::{bar, figure, structure};
use figures::{soft_shadow, structure_fallback};
pub use palette::{AMBER, GREEN, INK, MUTED, PANEL, RED, TEXT};
pub use projection::ArenaProjection;
#[cfg(test)]
pub use projection::{inverse, world};
pub use scenes::{camp_scene, country_scene_textured};
