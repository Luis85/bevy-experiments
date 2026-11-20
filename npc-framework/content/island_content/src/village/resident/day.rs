//! Every resident's own day (review npc-ai#11).
//!
//! The world's schedule (`behavior.schedule.blocks`) is one plan; each
//! resident keeps its own copy of it, [`OwnSchedule`], derived once from data
//! and saved with the resident:
//!
//! 1. its trade's [`DayShape`] (`economy.professions[].day`), then, for an
//!    elder, `behavior.schedule.elder`, each replacing the world's blocks of
//!    every kind it brings its own blocks for;
//! 2. the whole shift: chronotype x `chronotype_shift` / 100 plus every
//!    shape's `shift`, and each shape's edge moves for the block's kind;
//! 3. the overlap rule: the shared blocks (meals, and the world's own leisure)
//!    move by at most `align_permille`, so meals and free time stay shared;
//!    work, sleep and a shape's own blocks take the whole shift;
//! 4. leisure starts later for the diligent (`diligence_shift`); a block the
//!    delay or the edges swallow is left out.
//!
//! The schedule is derived when the resident's tree is assigned (founding,
//! settling, a change of trade, becoming an elder) and when the behaviour
//! data is replaced; every block check reads it, with no recomputation per
//! tick. An actor without one (a visitor) gets the same rule on the fly.
use super::{BlockKind, ScheduleBlock, ScheduleSpec};
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

/// Moves the edges of every block of one kind, in permille of the day.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockEdges {
    pub kind: BlockKind,
    /// The block starts this much later (earlier when negative).
    #[serde(default)]
    pub from: i32,
    /// The block ends this much later (earlier when negative).
    #[serde(default)]
    pub to: i32,
}

/// How a trade or a life stage shapes the day, as data.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DayShape {
    /// Permille the whole day moves later (earlier when negative).
    pub shift: i32,
    /// Edge moves of the blocks of a kind, after the shift.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub edges: Vec<BlockEdges>,
    /// Own blocks, in the world's permille before the shift, in place of the
    /// world's blocks of the same kinds (an innkeeper's afternoon off).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<ScheduleBlock>,
}

impl DayShape {
    pub fn is_empty(&self) -> bool {
        self.shift == 0 && self.edges.is_empty() && self.blocks.is_empty()
    }

    /// Bounds of a shape. Meals are shared: a shape neither moves their
    /// edges nor brings its own.
    pub fn validate(&self, what: &str) -> Result<(), String> {
        let small = |value: i32| (-200..=200).contains(&value);
        if !small(self.shift)
            || self.edges.len() > 8
            || self.blocks.len() > 8
            || self
                .edges
                .iter()
                .any(|edge| !small(edge.from) || !small(edge.to))
            || self.blocks.iter().any(|block| {
                !(0..=1000).contains(&block.from)
                    || !(0..=1000).contains(&block.to)
                    || block.from == block.to
            })
        {
            return Err(format!(
                "{what}: shift and edge moves in -200..=200 permille, at most 8 edges and 8 \
                 blocks, each block with from and to in 0..=1000 and from != to"
            ));
        }
        if self.edges.iter().any(|edge| edge.kind == BlockKind::Meal)
            || self
                .blocks
                .iter()
                .any(|block| block.kind == BlockKind::Meal)
        {
            return Err(format!(
                "{what}: meals are shared; a day shape cannot move or replace them"
            ));
        }
        Ok(())
    }

    pub fn sim_hash(&self, h: &mut Fnv64) {
        h.i32(self.shift);
        h.u32(self.edges.len() as u32);
        for edge in &self.edges {
            h.u8(edge.kind as u8);
            h.i32(edge.from);
            h.i32(edge.to);
        }
        h.u32(self.blocks.len() as u32);
        for block in &self.blocks {
            h.u8(block.kind as u8);
            h.i32(block.from);
            h.i32(block.to);
        }
    }
}

/// A resident's own day: the blocks every block check reads, in permille of
/// the day already moved (`from > to` wraps past dawn; `0..1000` is the whole
/// day). Derived from data, saved with the resident.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct OwnSchedule {
    pub blocks: Vec<ScheduleBlock>,
}

impl SimHash for OwnSchedule {
    const NAME: &'static str = "island_own_schedule_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.u32(self.blocks.len() as u32);
        for block in &self.blocks {
            h.u8(block.kind as u8);
            h.i32(block.from);
            h.i32(block.to);
        }
    }
}

/// The rule of the module docs: the own blocks of a resident with `traits`
/// whose day the `shapes` (trade first, then life stage) shape.
pub fn own_blocks(
    schedule: &ScheduleSpec,
    traits: super::ResidentTraits,
    shapes: &[&DayShape],
) -> Vec<ScheduleBlock> {
    // (block, shared): a block of the world's own schedule is shared.
    let mut blocks: Vec<(ScheduleBlock, bool)> =
        schedule.blocks.iter().map(|block| (*block, true)).collect();
    for shape in shapes.iter().filter(|shape| !shape.blocks.is_empty()) {
        blocks.retain(|(block, _)| shape.blocks.iter().all(|own| own.kind != block.kind));
        blocks.extend(shape.blocks.iter().map(|block| (*block, false)));
    }
    let shift = traits.chronotype * schedule.chronotype_shift / 100
        + shapes.iter().map(|shape| shape.shift).sum::<i32>();
    let align = schedule.align_permille;
    let delay = (traits.diligence - 50) * schedule.diligence_shift / 50;
    blocks
        .into_iter()
        .filter_map(|(block, shared)| {
            let (mut early, mut late) = (0, 0);
            for edge in shapes
                .iter()
                .flat_map(|shape| shape.edges.iter())
                .filter(|edge| edge.kind == block.kind)
            {
                early += edge.from;
                late += edge.to;
            }
            // `from < to` up to a whole day; `from > to` wraps past dawn.
            let mut length = if block.to > block.from {
                block.to - block.from
            } else {
                block.to + 1000 - block.from
            };
            let mut from = block.from + early;
            length += late - early;
            if block.kind == BlockKind::Leisure {
                from += delay;
                length -= delay;
            }
            if length <= 0 {
                return None;
            }
            if length >= 1000 {
                return Some(ScheduleBlock {
                    kind: block.kind,
                    from: 0,
                    to: 1000,
                });
            }
            let moved = if shared && matches!(block.kind, BlockKind::Meal | BlockKind::Leisure) {
                shift.clamp(-align, align)
            } else {
                shift
            };
            let from = (from + moved).rem_euclid(1000);
            Some(ScheduleBlock {
                kind: block.kind,
                from,
                to: (from + length).rem_euclid(1000),
            })
        })
        .collect()
}

/// The own blocks `actor` should have now, from the data, its traits, its
/// trade and its life stage; `None` without behaviour data.
pub fn derive(world: &World, actor: Entity) -> Option<Vec<ScheduleBlock>> {
    let spec = super::spec(world)?;
    let traits = super::traits(world, actor);
    let trade = world
        .get::<super::super::Villager>(actor)
        .and_then(|person| {
            super::super::frontier::economy_spec::registry(world)?
                .definition(person.role)
                .map(|definition| &definition.day)
        });
    let elder =
        super::super::demography::stage(world, actor) == super::super::demography::LifeStage::Elder;
    let mut shapes: Vec<&DayShape> = trade.into_iter().collect();
    if elder {
        shapes.push(&spec.schedule.elder);
    }
    Some(own_blocks(&spec.schedule, traits, &shapes))
}

/// Derives `actor`'s own day again and keeps it when it changed. Called when
/// its trade, its life stage or the behaviour data change.
pub fn refresh_schedule(world: &mut World, actor: Entity) {
    if world.get::<super::super::Villager>(actor).is_none() {
        return;
    }
    let Some(blocks) = derive(world, actor) else {
        return;
    };
    if world
        .get::<OwnSchedule>(actor)
        .is_some_and(|own| own.blocks == blocks)
    {
        return;
    }
    world.entity_mut(actor).insert(OwnSchedule { blocks });
}

/// The blocks of `actor`'s own day: its saved schedule, else (an actor that
/// was never given one) the same rule on the fly.
pub fn schedule_of(world: &World, actor: Entity) -> Cow<'_, [ScheduleBlock]> {
    match world.get::<OwnSchedule>(actor) {
        Some(own) => Cow::Borrowed(&own.blocks),
        None => Cow::Owned(derive(world, actor).unwrap_or_default()),
    }
}
