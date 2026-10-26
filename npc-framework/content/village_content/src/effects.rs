//! Cross-entity side effects. Action systems write `SimEffect` messages;
//! `NpcSet::Effects` applies them sorted by `(source, kind)` so the outcome is
//! independent of action-system iteration order.

use bevy_ecs::message::Messages;
use bevy_ecs::prelude::*;
use serde::{Deserialize, Serialize};

use npc_framework::kernel::JsonObject;
use npc_framework::kernel::journal::JournalKind;
use npc_framework::prelude::*;

use crate::world::Stockpile;

/// JSON: `{"type":"AddFood","stockpile":3,"amount":3}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum EffectKind {
    /// Add food to a stockpile, clamped to its capacity.
    AddFood { stockpile: SimId, amount: u32 },
    /// Remove food from a stockpile (saturating at 0).
    TakeFood { stockpile: SimId, amount: u32 },
}

/// Derived `Ord` compares `source` first, then `kind`: that is the apply order.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SimEffect {
    pub source: SimId,
    pub kind: EffectKind,
}

/// `NpcSet::Effects` system: drains every pending `SimEffect` (it does not rely
/// on `Messages::update`, which only runs in `First`), sorts them by
/// `(source, kind)` and applies them. Effects whose target is missing (no such
/// `SimId`, or no `Stockpile` on it) are dropped. Each effect is journaled
/// (`Effect`, with `applied`) in apply order.
pub fn resolve_effects_system(
    mut messages: ResMut<Messages<SimEffect>>,
    index: Res<SimIndex>,
    mut stockpiles: Query<&mut Stockpile>,
    journal: Option<ResMut<Journal>>,
    clock: Option<Res<SimClock>>,
) {
    if messages.is_empty() {
        return;
    }
    let mut journal = journal.filter(|j| j.enabled);
    let tick = clock.map_or(0, |c| c.tick);
    let mut effects: Vec<SimEffect> = messages.drain().collect();
    effects.sort_unstable();
    for effect in effects {
        let applied = match effect.kind {
            EffectKind::AddFood { stockpile, amount } => {
                stockpile_mut(&index, &mut stockpiles, stockpile).map(|mut pile| {
                    pile.food = pile.food.saturating_add(amount).min(pile.capacity);
                })
            }
            EffectKind::TakeFood { stockpile, amount } => {
                stockpile_mut(&index, &mut stockpiles, stockpile).map(|mut pile| {
                    pile.food = pile.food.saturating_sub(amount);
                })
            }
        }
        .is_some();
        if let Some(journal) = &mut journal {
            journal.push(
                tick,
                Some(effect.source),
                JournalKind::Effect {
                    effect: JsonObject::from_typed(&effect.kind)
                        .expect("EffectKind serializes to an object"),
                    applied,
                },
            );
        }
    }
}

fn stockpile_mut<'a>(
    index: &SimIndex,
    stockpiles: &'a mut Query<&mut Stockpile>,
    id: SimId,
) -> Option<Mut<'a, Stockpile>> {
    stockpiles.get_mut(index.get(id)?).ok()
}
