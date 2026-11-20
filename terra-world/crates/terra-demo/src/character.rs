//! Shared character presentation for creation previews and world avatars.

pub(crate) const SKIN_NAMES: [&str; 8] = [
    "Porcelain",
    "Ivory",
    "Sand",
    "Amber",
    "Copper",
    "Umber",
    "Cocoa",
    "Ebony",
];
pub(crate) const HAIR_NAMES: [&str; 8] = [
    "Black", "Chestnut", "Brown", "Auburn", "Honey", "Blond", "Silver", "White",
];
pub(crate) const OUTFIT_NAMES: [&str; 8] = [
    "Forest", "Ochre", "Slate", "Rust", "Ocean", "Plum", "Cream", "Charcoal",
];
pub(crate) const BODY_NAMES: [&str; 3] = ["Balanced", "Broad", "Slender"];

pub(crate) use terra_bevy::render::{
    character_appearance_colors as appearance_colors, character_body_scale as body_scale,
};
