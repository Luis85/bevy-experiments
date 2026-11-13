//! Visible goods: a sack, log bundle or crate on residents walking with bulk
//! goods, and a pile beside each workplace that grows with its stock. Props
//! are children of the existing view roots; inventories are only read, and a
//! workplace pile freezes while the fog is not showing that place live.
use std::collections::BTreeMap;

use bevy::{prelude::*, world_serialization::WorldAsset};
use npc_framework::prelude::{Inventory, Item, SimClock, SimIndex};
use npc_framework::view_helpers::{VisualIndex, VisualOf};
use testbed_content::village::{Role, VillagePlace, Villager, catalog};

use crate::fog::{Fogged, Look};
use crate::visuals::ActorVisual;

/// Fewest bulk units worth showing on someone's back.
pub(crate) const BULK_MIN: u32 = 2;
/// Most props in one workplace pile.
pub(crate) const MAX_PILE: usize = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum CarriedGood {
    Lumber,
    Crate,
    Sack,
    /// Sawn planks, for a builder: one plank shows (nobody keeps planks as
    /// fuel, as everybody keeps a log or two).
    Planks,
}

fn category(kind: &str) -> Option<CarriedGood> {
    Some(match kind {
        "log" => CarriedGood::Lumber,
        "plank" => CarriedGood::Planks,
        "ore" | "ingot" => CarriedGood::Crate,
        catalog::GRAIN
        | catalog::FLOUR
        | catalog::FIBER
        | catalog::HIDE
        | catalog::LEATHER
        | catalog::RAW_MEAT
        | catalog::FISH
        | catalog::BERRIES => CarriedGood::Sack,
        _ => return None,
    })
}

/// The dominant bulk good in an inventory, if there is enough of it to see.
/// Tools, coins, water, meals and clothing never count.
pub(crate) fn bulk_load<'a>(items: impl Iterator<Item = (&'a str, u32)>) -> Option<CarriedGood> {
    let mut units = [0_u32; 4];
    for (kind, stack) in items {
        if let Some(good) = category(kind) {
            units[good as usize] = units[good as usize].saturating_add(stack.max(1));
        }
    }
    [
        CarriedGood::Lumber,
        CarriedGood::Crate,
        CarriedGood::Sack,
        CarriedGood::Planks,
    ]
    .into_iter()
    .filter(|good| {
        units[*good as usize]
            >= if *good == CarriedGood::Planks {
                1
            } else {
                BULK_MIN
            }
    })
    .max_by_key(|good| (units[*good as usize], std::cmp::Reverse(*good)))
}

/// What a resident is seen carrying. A load shows on the move. Someone
/// standing at work, idle, resting or asleep has put it down: on a still
/// figure the prop covered the working arm, floated over sleepers and, since
/// every resident keeps a few logs of hearth fuel, was on everybody always.
pub(crate) fn shown_load(load: Option<CarriedGood>, moving: bool) -> Option<CarriedGood> {
    load.filter(|_| moving)
}

/// How many pile props a stock of `units` shows: none when empty, one for the
/// first few units, filling up to [`MAX_PILE`] around a full store.
pub(crate) fn pile_size(units: u32) -> usize {
    match units {
        0 => 0,
        1..=4 => 1,
        5..=10 => 2,
        11..=18 => 3,
        19..=28 => 4,
        29..=40 => 5,
        _ => MAX_PILE,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PileKind {
    Lumber,
    Crate,
    Sack,
    Barrel,
}

pub(crate) fn pile_kind(role: Role) -> PileKind {
    match role {
        Role::Woodcutter | Role::Carpenter => PileKind::Lumber,
        Role::Miner | Role::Smith => PileKind::Crate,
        Role::Innkeeper | Role::WaterKeeper => PileKind::Barrel,
        _ => PileKind::Sack,
    }
}

/// The prop a good is shown as where it lies in a store, on a stall or
/// beside a building site. Liquids and meals are barrels; anything else that
/// is not timber or a sack good is a crate.
pub(crate) fn stored_kind(kind: &str) -> PileKind {
    match category(kind) {
        Some(CarriedGood::Lumber | CarriedGood::Planks) => PileKind::Lumber,
        Some(CarriedGood::Sack) => PileKind::Sack,
        Some(CarriedGood::Crate) => PileKind::Crate,
        None if matches!(kind, catalog::WATER | catalog::COOKED_MEAL) => PileKind::Barrel,
        None => PileKind::Crate,
    }
}

/// Scale of a pile prop of each kind at its placed size.
pub(crate) fn prop_scale(kind: PileKind) -> f32 {
    match kind {
        PileKind::Lumber => 1.5,
        PileKind::Crate => 2.6,
        PileKind::Sack => 4.2,
        PileKind::Barrel => 2.8,
    }
}

/// Slot `i` of a pile, relative to the workplace root: beside the building,
/// clear of the spot residents work from. Three on the ground, two on top of
/// them and one more beside.
pub(crate) fn pile_slot(kind: PileKind, i: usize) -> Transform {
    let (scale, pitch, height) = match kind {
        PileKind::Lumber => (1.5, 0.56, 0.30),
        PileKind::Crate => (2.6, 0.62, 0.55),
        PileKind::Sack => (4.2, 0.56, 0.22),
        PileKind::Barrel => (2.8, 0.62, 0.0),
    };
    let (column, level) = match i {
        0..=2 => (i as f32, 0.0),
        3 | 4 if height > 0.0 => (i as f32 - 2.5, 1.0),
        3 | 4 => (i as f32, 0.0),
        _ => (if height > 0.0 { 3.0 } else { 5.0 }, 0.0),
    };
    let wide = column >= 3.0;
    Transform::from_xyz(
        2.35 + if wide { (column - 3.0) * pitch } else { 0.0 },
        level * height,
        if wide { 0.95 } else { 1.6 + column * pitch },
    )
    .with_rotation(Quat::from_rotation_y((i as f32 * 2.4).sin() * 0.12))
    .with_scale(Vec3::splat(scale))
}

/// How far a workplace's pile stands further out than [`pile_slot`] has it.
/// The slots were laid out beside the small models; a fitted workplace
/// fills its body, 2.5 m to each side, so its pile moves clear of the wall.
pub(crate) fn pile_shift(role: Role) -> f32 {
    if crate::island_world::workplaces::WorkModel::of(role).is_some() {
        0.75
    } else {
        0.0
    }
}

#[derive(Resource)]
pub(crate) struct GoodsAssets {
    lumber: Handle<WorldAsset>,
    planks: Handle<WorldAsset>,
    crate_model: Handle<WorldAsset>,
    sack: Handle<WorldAsset>,
    barrel: Handle<WorldAsset>,
}

#[derive(Component)]
pub(crate) struct CarryProp {
    shown: Option<CarriedGood>,
}
#[derive(Component)]
pub(crate) struct Pile {
    place: Entity,
    root: Entity,
    shown: usize,
}
#[derive(Component)]
pub(crate) struct PileProp(usize);

#[derive(Resource, Default)]
pub(crate) struct GoodsLinks {
    carriers: BTreeMap<Entity, Entity>,
    piles: BTreeMap<Entity, Entity>,
    tick: Option<u64>,
}

pub(crate) fn setup(mut commands: Commands, server: Res<AssetServer>) {
    let scene = |path: &'static str| server.load(GltfAssetLabel::Scene(0).from_asset(path));
    commands.insert_resource(GoodsAssets {
        lumber: scene("hex/resource_lumber.gltf"),
        planks: scene("testbed/survival/resource-planks.glb"),
        crate_model: scene("hex/crate_A_big.gltf"),
        sack: scene("hex/sack.gltf"),
        barrel: scene("hex/barrel.gltf"),
    });
}

fn carried_transform(good: CarriedGood) -> Transform {
    match good {
        // Over the right shoulder, pointing ahead.
        CarriedGood::Lumber => Transform::from_xyz(0.24, 1.52, 0.0)
            .with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2))
            .with_scale(Vec3::splat(0.85)),
        // A short stack of planks over the shoulder.
        CarriedGood::Planks => Transform::from_xyz(0.22, 1.48, 0.0)
            .with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2))
            .with_scale(Vec3::splat(1.6)),
        // On the back (models face +Z).
        CarriedGood::Crate => Transform::from_xyz(0.0, 0.95, -0.34).with_scale(Vec3::splat(1.5)),
        CarriedGood::Sack => Transform::from_xyz(0.0, 1.02, -0.30)
            .with_rotation(Quat::from_rotation_x(-1.25))
            .with_scale(Vec3::splat(3.4)),
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn sync(
    mut commands: Commands,
    assets: Option<Res<GoodsAssets>>,
    clock: Res<SimClock>,
    index: Res<SimIndex>,
    visuals: Res<VisualIndex>,
    mut links: ResMut<GoodsLinks>,
    actors: Query<(Entity, &VisualOf, &ActorVisual)>,
    people: Query<&Inventory, With<Villager>>,
    places: Query<(Entity, &VillagePlace, &Inventory)>,
    items: Query<&Item>,
    fog: Query<&Fogged>,
    alive: Query<(), With<Transform>>,
    mut carried: Query<(&mut CarryProp, &mut Transform, &mut Visibility), Without<PileProp>>,
    mut piles: Query<(&mut Pile, &Children)>,
    mut props: Query<(&PileProp, &mut Visibility), Without<CarryProp>>,
) {
    let Some(assets) = assets else { return };
    // Stock and loads change with sim ticks, not frames.
    if links.tick == Some(clock.tick) {
        return;
    }
    links.tick = Some(clock.tick);
    let stacks = |inventory: &'_ Inventory| -> Vec<(String, u32)> {
        inventory
            .items
            .iter()
            .filter_map(|id| {
                let item = items.get(index.get(*id)?).ok()?;
                Some((item.kind.clone(), item.stack))
            })
            .collect()
    };
    links
        .carriers
        .retain(|root, prop| alive.contains(*root) && alive.contains(*prop));
    links
        .piles
        .retain(|root, pile| alive.contains(*root) && alive.contains(*pile));
    for (root, of, actor) in &actors {
        let Ok(inventory) = people.get(of.0) else {
            continue;
        };
        let load = stacks(inventory);
        let good = shown_load(
            bulk_load(load.iter().map(|(kind, stack)| (kind.as_str(), *stack))),
            actor.motion.moving,
        );
        let prop = match links.carriers.get(&root) {
            Some(prop) => *prop,
            None => {
                if good.is_none() {
                    continue;
                }
                let prop = commands
                    .spawn((
                        Name::new("carried goods"),
                        ChildOf(root),
                        CarryProp { shown: None },
                        Transform::default(),
                        Visibility::Hidden,
                    ))
                    .id();
                links.carriers.insert(root, prop);
                // Its components exist next frame; the load is applied then.
                links.tick = None;
                continue;
            }
        };
        let Ok((mut state, mut transform, mut visibility)) = carried.get_mut(prop) else {
            continue;
        };
        if state.shown == good {
            continue;
        }
        state.shown = good;
        match good {
            Some(good) => {
                *transform = carried_transform(good);
                *visibility = Visibility::Inherited;
                commands.entity(prop).insert(WorldAssetRoot(match good {
                    CarriedGood::Lumber => assets.lumber.clone(),
                    CarriedGood::Planks => assets.planks.clone(),
                    CarriedGood::Crate => assets.crate_model.clone(),
                    CarriedGood::Sack => assets.sack.clone(),
                }));
            }
            None => *visibility = Visibility::Hidden,
        }
    }
    for (place, site, _) in &places {
        let Some(&root) = visuals
            .map
            .get(&place)
            .filter(|root| alive.contains(**root))
        else {
            continue;
        };
        if links.piles.contains_key(&root) {
            continue;
        }
        let kind = pile_kind(site.role);
        let pile = commands
            .spawn((
                Name::new("workplace stock"),
                ChildOf(root),
                Pile {
                    place,
                    root,
                    shown: 0,
                },
                Transform::from_xyz(pile_shift(site.role), 0.0, 0.0),
                Visibility::default(),
            ))
            .id();
        for i in 0..MAX_PILE {
            commands.spawn((
                ChildOf(pile),
                PileProp(i),
                WorldAssetRoot(match kind {
                    PileKind::Lumber => assets.lumber.clone(),
                    PileKind::Crate => assets.crate_model.clone(),
                    PileKind::Sack => assets.sack.clone(),
                    PileKind::Barrel => assets.barrel.clone(),
                }),
                pile_slot(kind, i),
                Visibility::Hidden,
            ));
        }
        links.piles.insert(root, pile);
        links.tick = None;
    }
    for (mut pile, children) in &mut piles {
        // Never update a remembered or dimmed place from live stock.
        if fog.get(pile.root).is_ok_and(|fog| fog.0 != Look::Live) {
            continue;
        }
        let Ok((_, _, inventory)) = places.get(pile.place) else {
            continue;
        };
        let units: u32 = stacks(inventory)
            .iter()
            .filter(|(kind, _)| kind != catalog::COINS && kind != catalog::DIARY)
            .map(|(_, stack)| (*stack).max(1))
            .sum();
        let size = pile_size(units);
        if size == pile.shown {
            continue;
        }
        pile.shown = size;
        for child in children.iter() {
            if let Ok((prop, mut visibility)) = props.get_mut(child) {
                *visibility = if prop.0 < size {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_enough_bulk_goods_show_and_the_larger_load_wins() {
        let load = |items: &[(&'static str, u32)]| bulk_load(items.iter().copied());
        assert_eq!(load(&[]), None);
        assert_eq!(load(&[("log", 1)]), None);
        assert_eq!(load(&[("log", 2)]), Some(CarriedGood::Lumber));
        // A builder's planks show from one; logs from two (everybody keeps
        // a log or two of fuel), and the larger load wins.
        assert_eq!(load(&[("log", 1), ("plank", 1)]), Some(CarriedGood::Planks));
        assert_eq!(load(&[("plank", 3)]), Some(CarriedGood::Planks));
        assert_eq!(load(&[("log", 4), ("plank", 1)]), Some(CarriedGood::Lumber));
        assert_eq!(stored_kind("plank"), PileKind::Lumber);
        assert_eq!(
            load(&[("log", 2), (catalog::GRAIN, 12)]),
            Some(CarriedGood::Sack)
        );
        assert_eq!(load(&[("ore", 3), ("ingot", 1)]), Some(CarriedGood::Crate));
        assert_eq!(load(&[("ore", 2), ("log", 2)]), Some(CarriedGood::Lumber));
        // Personal effects are never a visible load.
        assert_eq!(
            load(&[
                (catalog::COINS, 400),
                (catalog::WATER, 6),
                ("axe", 1),
                ("bread", 9),
                (catalog::WARM_CLOTHES, 2)
            ]),
            None
        );
    }

    #[test]
    fn a_load_shows_on_the_move_and_is_put_down_to_work_or_rest() {
        for good in [
            CarriedGood::Lumber,
            CarriedGood::Crate,
            CarriedGood::Sack,
            CarriedGood::Planks,
        ] {
            assert_eq!(shown_load(Some(good), true), Some(good));
            assert_eq!(shown_load(Some(good), false), None);
        }
        assert_eq!(shown_load(None, true), None);
    }

    #[test]
    fn piles_grow_with_stock_and_stay_bounded() {
        assert_eq!(pile_size(0), 0);
        assert_eq!(pile_size(1), 1);
        let mut previous = 0;
        for units in 0..200 {
            let size = pile_size(units);
            assert!(size >= previous && size <= MAX_PILE);
            previous = size;
        }
        assert_eq!(pile_size(41), MAX_PILE);
        assert_eq!(pile_size(u32::MAX), MAX_PILE);
    }

    #[test]
    fn a_fitted_workplaces_pile_stands_clear_of_its_wall() {
        // The body is 2.5 m to a side of the site; the widest prop, a
        // barrel, is under 0.4 m in radius.
        for role in [Role::Innkeeper, Role::Smith, Role::Baker, Role::Farmer] {
            for kind in [
                PileKind::Barrel,
                PileKind::Crate,
                PileKind::Sack,
                PileKind::Lumber,
            ] {
                for i in 0..MAX_PILE {
                    let x = pile_slot(kind, i).translation.x + pile_shift(role);
                    assert!(x - 0.4 >= 2.5, "{role:?} {kind:?} slot {i}: {x}");
                }
            }
        }
        // An open workplace keeps its pile beside its small model.
        assert_eq!(pile_shift(Role::WaterKeeper), 0.0);
        assert_eq!(pile_shift(Role::Fisher), 0.0);
    }

    #[test]
    fn pile_slots_do_not_overlap_and_keep_the_work_spot_clear() {
        for kind in [
            PileKind::Lumber,
            PileKind::Crate,
            PileKind::Sack,
            PileKind::Barrel,
        ] {
            let slots: Vec<Vec3> = (0..MAX_PILE)
                .map(|i| pile_slot(kind, i).translation)
                .collect();
            for (i, a) in slots.iter().enumerate() {
                assert!(a.x >= 2.0, "{kind:?} slot {i} crowds the work spot");
                for b in &slots[i + 1..] {
                    assert!(a.distance(*b) > 0.2, "{kind:?} slots overlap");
                }
            }
        }
        assert_eq!(pile_kind(Role::Woodcutter), PileKind::Lumber);
        assert_eq!(pile_kind(Role::Smith), PileKind::Crate);
        assert_eq!(pile_kind(Role::Innkeeper), PileKind::Barrel);
        assert_eq!(pile_kind(Role::Farmer), PileKind::Sack);
    }
}
