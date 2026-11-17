mod backdrop;
mod battlefield;
pub mod camera;
mod figures;
pub mod lighting;
mod palette;
mod projection;
mod scenes;
#[cfg(test)]
mod tests;

pub use backdrop::world_backdrop;
use backdrop::{box_at, line, paint_ground};
#[cfg(test)]
pub(crate) use battlefield::reinforcements_anchor;
#[cfg(feature = "review")]
pub use battlefield::{FlashFacts, flash_facts};
#[cfg(all(test, feature = "review"))]
pub(crate) use battlefield::{FxCounts, burst_keyframe_parts, fx_counts};
pub use battlefield::{FxSource, shooter_class};
pub(crate) use battlefield::{
    HEALTH_BAR_LIFT, OCCLUDER_GAP, coin_rects, crit_plate, cue_label_sizes, enemy_body_box,
    fx_lifetime, incoming_rects, midpoint_label_rect, structure_face,
};
#[cfg(test)]
pub(crate) use battlefield::{
    MIDPOINT_LABEL, MIDPOINT_LABEL_MIN_WRAP, MIDPOINT_LABEL_SIZE, midpoint_label_place,
};
pub use battlefield::{RenderOptions, battlefield, battlefield_for_attempt, sample_motion};
pub use camera::{FieldFit, fit_field};
pub use figures::{bar, figure, structure};
use figures::{soft_shadow, structure_fallback};
pub use lighting::{SceneLook, SkyClock, scene_look};
pub use palette::{AMBER, GREEN, INK, MUTED, PANEL, RED, TEXT};
pub use projection::ArenaProjection;
#[cfg(test)]
pub use projection::{inverse, world};
pub use scenes::{camp_scene, country_scene_textured};
