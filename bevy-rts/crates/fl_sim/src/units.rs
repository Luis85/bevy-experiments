//! Core unit def components (`"Selectable"`, `"Hull"`, `"Mobility"`) and the runtime
//! components every unit carries.
//!
//! Defs hold authoring units (km/h, degrees per second); `instantiate` converts them once,
//! at spawn, into per-tick `Fx` and [`Angle`] values.

use std::any::Any;

use bevy_ecs::component::Component;
use bevy_ecs::world::EntityWorldMut;
use fl_content::{DefComponent, LinkCtx, MoveClass, SpawnCtx};
use fl_math::{Angle, Fx, FxVec2};
use fl_protocol::{PlayerId, SIM_TICK_HZ, SimId, UnitTypeId, tables};
use serde::{Deserialize, Serialize};

use crate::builder::SimBuilder;
use crate::classify::{Class, Codec, from_postcard, trace_type};
use crate::hash::{SimHash, StateHasher};
use crate::path::PathFollow;
use crate::save::{LoadCtx, SaveCtx};

/// km/h to metres per tick is `kmh · 10 / (36 · SIM_TICK_HZ)` (`kmh / 72` at 20 Hz). The
/// two integers are exact at every tick rate; only the final `Fx` division rounds.
const KMH_TO_M_PER_TICK_NUM: i32 = 10;
const KMH_TO_M_PER_TICK_DEN: i32 = 36 * SIM_TICK_HZ as i32;

/// `kmh` as metres per tick.
fn speed_per_tick(kmh: Fx) -> Fx {
    kmh.mul_int(KMH_TO_M_PER_TICK_NUM) / Fx::from_int(KMH_TO_M_PER_TICK_DEN)
}

/// `deg_s` degrees per second as the angle turned per tick, rounded to the nearest unit.
fn turn_per_tick(deg_s: Fx) -> Angle {
    Angle::from_degrees(deg_s / Fx::from_int(SIM_TICK_HZ as i32))
}

/// The largest footprint side, in metres.
const MAX_FOOTPRINT_M: i32 = 50;
/// The fastest road speed, in km/h.
const MAX_ROAD_SPEED_KMH: i32 = 200;
/// The fastest turn rate, in degrees per second.
const MAX_TURN_RATE_DEG_S: i32 = 720;

/// `"Selectable": ()`: the player can select the unit. Adds [`Selectable`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectableDef;

impl DefComponent for SelectableDef {
    fn instantiate(&self, _ctx: &SpawnCtx<'_>, entity: &mut EntityWorldMut<'_>) {
        entity.insert(Selectable);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// `"Hull": (hp, footprint: (width, length), hitbox_shrink)`. Adds [`Health`] and
/// [`Footprint`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HullDef {
    pub hp: u32,
    /// `(width, length)` in metres; the length runs along the unit's heading.
    pub footprint: FxVec2,
    /// How much smaller than the footprint the hitbox is, 0–1.
    pub hitbox_shrink: Fx,
}

impl HullDef {
    fn footprint(&self) -> Footprint {
        Footprint {
            width: self.footprint.x,
            length: self.footprint.y,
            hitbox_shrink: self.hitbox_shrink,
        }
    }
}

/// `hp` of `old_max` as hit points of `new_max`, keeping the ratio (rounded down): 0 stays 0
/// (a dead hull stays dead), any other value is in `1..=new_max` (hp above the old maximum,
/// from a debug edit, is clamped). Without an old maximum the hull is whole.
///
/// The floor drifts down over repeated reloads: 99/100 → 49/50 → 98/100. That is the rule
/// ADR-0012 pins (integer, deterministic); a reload is a testbed tool, so the drift is
/// accepted.
fn rescale_hp(hp: u32, old_max: u32, new_max: u32) -> u32 {
    if hp == 0 {
        return 0;
    }
    if old_max == 0 {
        return new_max;
    }
    let scaled = u64::from(hp) * u64::from(new_max) / u64::from(old_max);
    u32::try_from(scaled)
        .unwrap_or(u32::MAX)
        .clamp(1, new_max.max(1))
}

impl DefComponent for HullDef {
    fn validate(&self, ctx: &mut LinkCtx<'_>) {
        if self.hp < 1 {
            ctx.error("hp", "must be at least 1");
        }
        let max = Fx::from_int(MAX_FOOTPRINT_M);
        for (axis, side) in [("width", self.footprint.x), ("length", self.footprint.y)] {
            if side <= Fx::ZERO || side > max {
                ctx.error(
                    "footprint",
                    format!("{axis} {side} m must be in (0, {MAX_FOOTPRINT_M}] m"),
                );
            }
        }
        if self.hitbox_shrink < Fx::ZERO || self.hitbox_shrink > Fx::ONE {
            ctx.error(
                "hitbox_shrink",
                format!("{} must be in [0, 1]", self.hitbox_shrink),
            );
        }
    }

    fn instantiate(&self, _ctx: &SpawnCtx<'_>, entity: &mut EntityWorldMut<'_>) {
        entity.insert((
            Health {
                hp: self.hp,
                max: self.hp,
            },
            self.footprint(),
        ));
    }

    /// Keeps the HP ratio ([`rescale_hp`]) and replaces the [`Footprint`].
    fn reload(&self, _ctx: &SpawnCtx<'_>, entity: &mut EntityWorldMut<'_>) {
        let hp = entity
            .get::<Health>()
            .map_or(self.hp, |old| rescale_hp(old.hp, old.max, self.hp));
        entity.insert((Health { hp, max: self.hp }, self.footprint()));
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// `"Mobility": (class, road_speed_kmh, terrain_factor, turn_rate_deg_s)`. Adds
/// [`Mobility`], an empty [`MoveOrder`] and an idle [`PathFollow`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MobilityDef {
    pub class: MoveClass,
    pub road_speed_kmh: Fx,
    /// How much of the terrain's slowdown the unit escapes, in [−1, 1] (ADR-0014): its speed
    /// on a cell is `clamp(pass + (1 − pass) × terrain_factor, Movement.min_speed_frac, 1)`
    /// of road speed. Negative values (trucks) slow it below the terrain's pass.
    pub terrain_factor: Fx,
    pub turn_rate_deg_s: Fx,
}

impl MobilityDef {
    fn mobility(&self) -> Mobility {
        Mobility {
            class: self.class,
            speed: speed_per_tick(self.road_speed_kmh),
            terrain_factor: self.terrain_factor,
            turn_rate: turn_per_tick(self.turn_rate_deg_s),
        }
    }
}

impl DefComponent for MobilityDef {
    fn validate(&self, ctx: &mut LinkCtx<'_>) {
        if self.road_speed_kmh <= Fx::ZERO || self.road_speed_kmh > Fx::from_int(MAX_ROAD_SPEED_KMH)
        {
            ctx.error(
                "road_speed_kmh",
                format!(
                    "{} km/h must be in (0, {MAX_ROAD_SPEED_KMH}] km/h",
                    self.road_speed_kmh
                ),
            );
        } else if speed_per_tick(self.road_speed_kmh) <= Fx::ZERO {
            ctx.error(
                "road_speed_kmh",
                format!(
                    "{} km/h is 0 m per tick at {SIM_TICK_HZ} Hz",
                    self.road_speed_kmh
                ),
            );
        }
        if self.terrain_factor < -Fx::ONE || self.terrain_factor > Fx::ONE {
            ctx.error(
                "terrain_factor",
                format!("{} must be in [-1, 1]", self.terrain_factor),
            );
        }
        if self.turn_rate_deg_s <= Fx::ZERO
            || self.turn_rate_deg_s > Fx::from_int(MAX_TURN_RATE_DEG_S)
        {
            ctx.error(
                "turn_rate_deg_s",
                format!(
                    "{} °/s must be in (0, {MAX_TURN_RATE_DEG_S}] °/s",
                    self.turn_rate_deg_s
                ),
            );
        } else if turn_per_tick(self.turn_rate_deg_s) == Angle::ZERO {
            ctx.error(
                "turn_rate_deg_s",
                format!(
                    "{} °/s rounds to no turn per tick at {SIM_TICK_HZ} Hz",
                    self.turn_rate_deg_s
                ),
            );
        }
    }

    fn instantiate(&self, _ctx: &SpawnCtx<'_>, entity: &mut EntityWorldMut<'_>) {
        entity.insert((self.mobility(), MoveOrder::default(), PathFollow::default()));
    }

    /// Replaces [`Mobility`] only: the unit keeps its [`MoveOrder`] and [`PathFollow`] (a unit
    /// that had none gets empty ones, as at spawn).
    fn reload(&self, _ctx: &SpawnCtx<'_>, entity: &mut EntityWorldMut<'_>) {
        entity.insert(self.mobility());
        if !entity.contains::<MoveOrder>() {
            entity.insert(MoveOrder::default());
        }
        if !entity.contains::<PathFollow>() {
            entity.insert(PathFollow::default());
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// The player that owns the unit.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Owner(pub PlayerId);

/// The unit's interned content id.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitType(pub UnitTypeId);

/// Where the unit is (metres) and which way it faces.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pose {
    pub pos: FxVec2,
    pub heading: Angle,
}

/// Hit points.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Health {
    pub hp: u32,
    pub max: u32,
}

/// The unit's ground rectangle, in metres.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Footprint {
    pub width: Fx,
    pub length: Fx,
    pub hitbox_shrink: Fx,
}

/// How the unit moves, in per-tick units.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mobility {
    pub class: MoveClass,
    /// Road speed in metres per tick (`km/h / 72` at 20 Hz).
    pub speed: Fx,
    pub terrain_factor: Fx,
    /// The most the heading turns in one tick (`°/s / 20`).
    pub turn_rate: Angle,
}

/// The unit's movement orders, in order; `orders[0]` is the current one. Hashed
/// `"core.move_order"` v2 (v1 held only move targets, `waypoints`, and migrates to
/// [`Order::Move`]s). Holds at most [`MAX_WAYPOINTS`](crate::MAX_WAYPOINTS) orders of every
/// kind (R77).
#[derive(Component, Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveOrder {
    #[serde(deserialize_with = "capped")]
    pub orders: Vec<Order>,
}

/// A sequence of at most [`MAX_WAYPOINTS`](crate::MAX_WAYPOINTS) elements: R77 holds for loaded
/// orders too, so a save holding more is a codec error.
fn capped<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    let items = Vec::<T>::deserialize(deserializer)?;
    if items.len() > crate::MAX_WAYPOINTS {
        return Err(serde::de::Error::custom(format!(
            "{} orders: at most {} fit a unit's queue",
            items.len(),
            crate::MAX_WAYPOINTS
        )));
    }
    Ok(items)
}

/// One movement order. A closed enum of core orders for M1 only (R69): the order queue and
/// activity stack of the M2 prelude (C7) replace it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Order {
    /// Drive to `target`, along a planned path.
    Move { target: FxVec2 },
    /// Turn in place to face `facing`.
    Rotate { facing: Angle },
    /// Keep `offset` from unit `leader`.
    Follow { leader: SimId, offset: FxVec2 },
}

impl Order {
    /// The order's kind as the journal names it: `"move"`, `"rotate"` or `"follow"`.
    pub(crate) fn kind_name(&self) -> &'static str {
        match self {
            Order::Move { .. } => "move",
            Order::Rotate { .. } => "rotate",
            Order::Follow { .. } => "follow",
        }
    }
}

/// The save formats of older versions, kept for as long as such saves must load.
mod v1 {
    use fl_math::FxVec2;
    use serde::Deserialize;

    /// `"core.move_order"` v1: the move targets, in order (at most `MAX_WAYPOINTS`).
    #[derive(Deserialize)]
    pub(super) struct MoveOrder {
        #[serde(deserialize_with = "super::capped")]
        pub(super) waypoints: Vec<FxVec2>,
    }
}

/// `"core.move_order"` v2 exactly as at tag `m1`, frozen: the order queue of the M2 prelude
/// (C7) changes [`MoveOrder`] and migrates saves from these types. Never edited again, and
/// self-contained: it copies the cap and its check, so nothing live can change how it decodes.
#[allow(
    dead_code,
    reason = "read by the v2 to order-queue migration of M2 prelude Task 13"
)]
pub(crate) mod v2 {
    use fl_math::{Angle, FxVec2};
    use fl_protocol::SimId;
    use serde::Deserialize;

    /// `MAX_WAYPOINTS` as at tag `m1`.
    const MAX_ORDERS: usize = 32;

    /// `capped` as at tag `m1`: more than [`MAX_ORDERS`] orders is a codec error.
    fn capped<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
    where
        D: serde::Deserializer<'de>,
        T: Deserialize<'de>,
    {
        let items = Vec::<T>::deserialize(deserializer)?;
        if items.len() > MAX_ORDERS {
            return Err(serde::de::Error::custom(format!(
                "{} orders: at most {MAX_ORDERS} fit a unit's queue",
                items.len()
            )));
        }
        Ok(items)
    }

    /// `"core.move_order"` v2: the orders, in order (at most 32).
    #[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
    pub(crate) struct MoveOrder {
        #[serde(deserialize_with = "capped")]
        pub(crate) orders: Vec<Order>,
    }

    /// One v2 movement order.
    #[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
    pub(crate) enum Order {
        Move { target: FxVec2 },
        Rotate { facing: Angle },
        Follow { leader: SimId, offset: FxVec2 },
    }
}

/// Every v1 waypoint becomes an [`Order::Move`]. v1 decoding rejects more than
/// `MAX_WAYPOINTS` waypoints, so the orders stay within the cap.
impl From<v1::MoveOrder> for MoveOrder {
    fn from(old: v1::MoveOrder) -> Self {
        MoveOrder {
            orders: old
                .waypoints
                .into_iter()
                .map(|target| Order::Move { target })
                .collect(),
        }
    }
}

/// Random movement around `home` (debug and bench behaviour): whenever the unit has no
/// order it is given a move to a point in the square `home ± radius`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wander {
    pub home: FxVec2,
    pub radius: Fx,
}

/// The player can select this unit.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Selectable;

impl SimHash for Owner {
    fn sim_hash(&self, h: &mut StateHasher) {
        self.0.sim_hash(h);
    }
}

impl SimHash for UnitType {
    fn sim_hash(&self, h: &mut StateHasher) {
        self.0.sim_hash(h);
    }
}

impl SimHash for Pose {
    fn sim_hash(&self, h: &mut StateHasher) {
        self.pos.sim_hash(h);
        self.heading.sim_hash(h);
    }
}

impl SimHash for Health {
    fn sim_hash(&self, h: &mut StateHasher) {
        self.hp.sim_hash(h);
        self.max.sim_hash(h);
    }
}

impl SimHash for Footprint {
    fn sim_hash(&self, h: &mut StateHasher) {
        self.width.sim_hash(h);
        self.length.sim_hash(h);
        self.hitbox_shrink.sim_hash(h);
    }
}

impl SimHash for MoveClass {
    fn sim_hash(&self, h: &mut StateHasher) {
        h.write_u8(match self {
            MoveClass::Foot => 0,
            MoveClass::Wheeled => 1,
            MoveClass::HalfTrack => 2,
            MoveClass::Tracked => 3,
        });
    }
}

impl SimHash for Mobility {
    fn sim_hash(&self, h: &mut StateHasher) {
        self.class.sim_hash(h);
        self.speed.sim_hash(h);
        self.terrain_factor.sim_hash(h);
        self.turn_rate.sim_hash(h);
    }
}

/// No data: presence is the state.
impl SimHash for Selectable {
    fn sim_hash(&self, _h: &mut StateHasher) {}
}

impl SimHash for MoveOrder {
    fn sim_hash(&self, h: &mut StateHasher) {
        self.orders.sim_hash(h);
    }
}

/// A tag byte (0 move, 1 rotate, 2 follow), then the fields.
impl SimHash for Order {
    fn sim_hash(&self, h: &mut StateHasher) {
        match self {
            Order::Move { target } => {
                h.write_u8(0);
                target.sim_hash(h);
            }
            Order::Rotate { facing } => {
                h.write_u8(1);
                facing.sim_hash(h);
            }
            Order::Follow { leader, offset } => {
                h.write_u8(2);
                leader.sim_hash(h);
                offset.sim_hash(h);
            }
        }
    }
}

impl SimHash for Wander {
    fn sim_hash(&self, h: &mut StateHasher) {
        self.home.sim_hash(h);
        self.radius.sim_hash(h);
    }
}

/// Saves the dense id, valid in the save's [`tables::UNITS`].
fn encode_unit_type(unit: &UnitType, _ctx: &SaveCtx<'_>) -> Vec<u8> {
    postcard::to_allocvec(&unit.0.0).expect("a u16 serializes")
}

/// The saved dense id, remapped by name to this sim's [`tables::UNITS`].
fn decode_unit_type(_version: u32, bytes: &[u8], ctx: &LoadCtx<'_>) -> Result<UnitType, String> {
    let saved: u16 = from_postcard(bytes)?;
    let current = ctx.remap(tables::UNITS, u32::from(saved))?;
    let id = u16::try_from(current).map_err(|_| format!("unit id {current} does not fit a u16"))?;
    Ok(UnitType(UnitTypeId(id)))
}

/// The unit def components and the unit components, all `Hashed` (v1, [`MoveOrder`] v2).
/// [`UnitType`] holds a dense content id, so its codec remaps it through the string tables on
/// load.
pub(crate) fn build(sim: &mut SimBuilder) {
    sim.register_def_component::<SelectableDef>("Selectable")
        .register_def_component::<HullDef>("Hull")
        .register_def_component::<MobilityDef>("Mobility")
        .register_component::<Owner>("core.owner", Class::hashed(1))
        .register_component::<UnitType>(
            "core.unit_type",
            Class::Hashed(Codec::custom(
                1,
                encode_unit_type,
                decode_unit_type,
                trace_type::<u16>,
            )),
        )
        .register_component::<Pose>("core.pose", Class::hashed(1))
        .register_component::<Health>("core.health", Class::hashed(1))
        .register_component::<Footprint>("core.footprint", Class::hashed(1))
        .register_component::<Mobility>(
            "core.mobility",
            Class::Hashed(Codec::new(1).trace_enum::<MoveClass>()),
        )
        .register_component::<MoveOrder>(
            "core.move_order",
            Class::Hashed(
                Codec::new(2)
                    .migrate_from::<v1::MoveOrder>(1)
                    .trace_enum::<Order>(),
            ),
        )
        .register_component::<Wander>("core.wander", Class::hashed(1))
        .register_component::<Selectable>("core.selectable", Class::hashed(1));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The v2 encoding, pinned as bytes written by the m1 build: Move (1, 2), Rotate 90°,
    /// Follow #3 (0, -8). Independent of the live [`MoveOrder`], so it keeps passing unchanged
    /// when the order queue replaces it.
    const V2_BYTES: [u8; 25] = [
        3, 0, 128, 128, 128, 128, 32, 128, 128, 128, 128, 64, 1, 128, 128, 1, 2, 3, 0, 255, 255,
        255, 255, 255, 1,
    ];

    #[test]
    fn frozen_v2_reads_current_move_order_bytes() {
        let xy = |x, y| FxVec2::new(Fx::from_int(x), Fx::from_int(y));
        let frozen: v2::MoveOrder = postcard::from_bytes(&V2_BYTES).expect("v2 decodes");
        assert_eq!(
            frozen.orders,
            [
                v2::Order::Move { target: xy(1, 2) },
                v2::Order::Rotate {
                    facing: Angle::QUARTER
                },
                v2::Order::Follow {
                    leader: SimId(3),
                    offset: xy(0, -8)
                },
            ]
        );
        // 33 orders are over the cap.
        let mut over = vec![33u8];
        over.extend(std::iter::repeat_n([1u8, 0].into_iter(), 33).flatten());
        assert!(postcard::from_bytes::<v2::MoveOrder>(&over).is_err());
    }

    #[test]
    fn kmh_divisor_is_72_at_20_hz() {
        assert_eq!(KMH_TO_M_PER_TICK_DEN / KMH_TO_M_PER_TICK_NUM, 72);
        assert_eq!(KMH_TO_M_PER_TICK_DEN % KMH_TO_M_PER_TICK_NUM, 0);
        assert_eq!(speed_per_tick(Fx::from_int(72)), Fx::ONE);
        assert_eq!(speed_per_tick(Fx::from_int(36)), Fx::HALF);
    }

    #[test]
    fn hp_rescale_keeps_ratio() {
        assert_eq!(rescale_hp(50, 100, 200), 100);
        assert_eq!(rescale_hp(100, 100, 50), 50);
        assert_eq!(rescale_hp(0, 100, 200), 0, "a dead hull stays dead");
        assert_eq!(rescale_hp(1, 100, 50), 1, "a live hull keeps at least 1 hp");
        assert_eq!(rescale_hp(33, 100, 10), 3, "rounded down");
        assert_eq!(rescale_hp(5, 0, 80), 80);
        assert_eq!(
            rescale_hp(150, 100, 100),
            100,
            "never above the new maximum"
        );
        assert_eq!(rescale_hp(150, 100, 300), 300);
        // Floor rounding drifts down over repeated reloads (documented).
        assert_eq!(rescale_hp(rescale_hp(99, 100, 50), 50, 100), 98);
        assert_eq!(
            rescale_hp(u32::MAX, u32::MAX, u32::MAX),
            u32::MAX,
            "no overflow"
        );
    }

    #[test]
    fn tiny_rates_round_to_zero_per_tick() {
        assert_eq!(speed_per_tick(Fx::from_raw(71)), Fx::ZERO);
        assert!(speed_per_tick(Fx::from_raw(72)) > Fx::ZERO);
        assert_eq!(turn_per_tick(Fx::from_ratio(1, 1000)), Angle::ZERO);
        assert_ne!(turn_per_tick(Fx::ONE), Angle::ZERO);
    }
}
