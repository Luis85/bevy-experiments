use crate::art::{AMBER, MUTED, TEXT};
use bevy_egui::egui::{self, Color32};
use horde_core::{campaign::Skill, content::StructureKind};
use std::collections::BTreeSet;

/// "Need N more coins": readable, but quieter than an error.
pub(in crate::ui::progression) const MUTED_RED: Color32 = Color32::from_rgb(201, 118, 100);
pub(super) const CANVAS: Color32 = Color32::from_rgb(12, 17, 17);

#[derive(Clone, Copy)]
pub(in crate::ui::progression) enum Icon {
    Skill(Skill),
    Firearm,
    Grenade,
    Flame,
    /// Endurance (stamina): a lightning bolt.
    Stamina,
    /// Perception (weak spots): an aiming sight.
    Sight,
    /// Companions and world: two survivors side by side.
    Companion,
    Defense(StructureKind, u32),
}

/// How a node reads on the canvas; derived from core `NodeState`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui::progression) enum Tone {
    Owned,
    Buyable,
    NeedsCoins,
    Prerequisite,
    Milestone,
    Unavailable,
}
impl Tone {
    pub(super) fn locked(self) -> bool {
        matches!(
            self,
            Self::Prerequisite | Self::Milestone | Self::Unavailable
        )
    }
    pub(super) fn badge_color(self) -> Color32 {
        match self {
            Self::Owned | Self::Buyable => AMBER,
            Self::NeedsCoins => MUTED_RED,
            _ => MUTED,
        }
    }
    pub(super) fn dot_color(self) -> Color32 {
        match self {
            Self::Owned => AMBER,
            Self::Buyable => TEXT,
            Self::NeedsCoins => MUTED_RED,
            _ => MUTED.gamma_multiply(0.55),
        }
    }
}

pub(in crate::ui::progression) struct Node {
    pub position: egui::Vec2,
    pub title: String,
    /// Short canvas caption: "OWNED", "40 c", "40 c · need 15", "Locked", "Win 3".
    pub badge: String,
    pub tone: Tone,
    pub icon: Icon,
    pub prerequisites: Vec<usize>,
}

/// Per-frame emphasis the screen computed from core queries.
#[derive(Default)]
pub(in crate::ui::progression) struct Emphasis {
    /// The selected node's unowned chain, the target included.
    pub path: BTreeSet<usize>,
    /// Search matches while a query is active; everything else dims.
    pub matches: Option<BTreeSet<usize>>,
    /// A just-learned node and its pulse phase in 0..1.
    pub pulse: Option<(usize, f32)>,
}

/// Blend `color` toward the canvas so faded icons stay opaque over wires.
pub(super) fn fade_fill(color: Color32, fade: f32) -> Color32 {
    let mix = |a: u8, b: u8| (f32::from(b) + (f32::from(a) - f32::from(b)) * fade).round() as u8;
    Color32::from_rgb(
        mix(color.r(), CANVAS.r()),
        mix(color.g(), CANVAS.g()),
        mix(color.b(), CANVAS.b()),
    )
}
