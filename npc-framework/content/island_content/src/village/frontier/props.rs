//! Props: benches, barrels, fences and the like that the overseer places with
//! the Shape tools (roadmap phase 4).
//!
//! What a prop is comes from the catalog data (`economy.props`: key, name,
//! footprint, whether it blocks movement, the model the view draws), so a new
//! prop needs no code. A placed prop is simulation data: it holds its cells
//! like a building (nobody builds over it), a solid one blocks walking, and
//! it is part of the exported world.
use super::building_spec::{
    BuildSurface, BuildingDef, BuildingKind, FootprintCells, PlacementRules,
};
use super::world_spec::PlacedProp;
use super::{SettlementMember, economy_spec};
use crate::village::civic::Plot;
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use npc_framework::kernel::commands::CommandError;
use npc_framework::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// One prop of the catalog.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PropDef {
    /// Stable key (`barrel`). Renaming it is a new prop.
    pub id: String,
    /// Name shown to players (`Barrel`).
    pub display: String,
    /// Cells it covers: `width` across its front, `depth` behind it.
    pub footprint: FootprintCells,
    /// A solid prop blocks walking; an open one (a flower bed) does not.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub solid: bool,
    /// The model the view draws, relative to the asset root; empty: the
    /// view's generic prop marker.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub model: String,
    /// Materials its construction site needs (build mode); empty with
    /// `work_ticks` 0: it stands at once.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cost: Vec<super::economy_spec::Stack>,
    /// Builder ticks after the materials are in.
    #[serde(default)]
    pub work_ticks: u32,
}
fn yes() -> bool {
    true
}
#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_true(value: &bool) -> bool {
    *value
}

/// The shipped props, each drawn by a model the asset manifest lists.
pub fn default_props() -> Vec<PropDef> {
    let prop = |id: &str,
                display: &str,
                (width, depth): (u32, u32),
                solid: bool,
                model: &str,
                cost: &[(&str, u32)],
                work_ticks: u32| PropDef {
        id: id.into(),
        display: display.into(),
        footprint: FootprintCells { width, depth },
        solid,
        model: model.into(),
        cost: cost
            .iter()
            .map(|(good, count)| super::economy_spec::Stack {
                good: (*good).into(),
                count: *count,
            })
            .collect(),
        work_ticks,
    };
    vec![
        prop(
            "barrel",
            "Barrel",
            (1, 1),
            true,
            "living/props/barrel.gltf",
            &[("plank", 1)],
            60,
        ),
        prop(
            "crate_stack",
            "Crate stack",
            (1, 1),
            true,
            "living/props/crate_A_big.gltf",
            &[("plank", 2)],
            80,
        ),
        prop(
            "fence",
            "Fence piece",
            (2, 1),
            true,
            "living/props/fence_wood_straight.gltf",
            &[("log", 1)],
            60,
        ),
        prop(
            "flower_bed",
            "Flower bed",
            (2, 1),
            false,
            "living/nature/flower_redA.glb",
            &[],
            60,
        ),
        prop(
            "signpost",
            "Signpost",
            (1, 1),
            true,
            "living/props/sign.glb",
            &[("plank", 1)],
            60,
        ),
        prop(
            "flag",
            "Flag",
            (1, 1),
            true,
            "living/props/flag_blue.gltf",
            &[("plank", 1)],
            60,
        ),
        prop(
            "wheelbarrow",
            "Wheelbarrow",
            (1, 1),
            true,
            "living/props/wheelbarrow.gltf",
            &[("plank", 2)],
            100,
        ),
    ]
}

/// Every error names the offending prop.
pub fn validate(economy: &economy_spec::EconomySpec) -> Result<(), String> {
    let mut seen = std::collections::BTreeSet::new();
    for def in &economy.props {
        if def.cost.len() > 4
            || def.cost.iter().any(|stack| {
                economy.good(&stack.good).is_none() || !(1..=100).contains(&stack.count)
            })
            || def.work_ticks > 10_000
        {
            return Err(format!(
                "economy: prop `{}` needs at most 4 known cost goods of 1..=100 each and work_ticks 0..=10000",
                def.id
            ));
        }
        let valid_key = !def.id.is_empty()
            && def
                .id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
        if !valid_key {
            return Err(format!(
                "economy: prop `{}` needs a key of lower-case letters, digits and `_`",
                def.id
            ));
        }
        if !seen.insert(def.id.as_str()) {
            return Err(format!("economy: duplicate prop `{}`", def.id));
        }
        if def.display.trim().is_empty() {
            return Err(format!("economy: prop `{}` needs a display name", def.id));
        }
        if !(1..=4).contains(&def.footprint.width) || !(1..=4).contains(&def.footprint.depth) {
            return Err(format!(
                "economy: prop `{}` footprint must be 1..=4 cells each way",
                def.id
            ));
        }
    }
    Ok(())
}

pub fn sim_hash(props: &[PropDef], h: &mut Fnv64) {
    h.str("props");
    h.u32(props.len() as u32);
    for def in props {
        h.str(&def.id);
        h.str(&def.display);
        h.u32(def.footprint.width);
        h.u32(def.footprint.depth);
        h.u8(u8::from(def.solid));
        h.str(&def.model);
        h.u32(def.cost.len() as u32);
        for stack in &def.cost {
            h.str(&stack.good);
            h.u32(stack.count);
        }
        h.u32(def.work_ticks);
    }
}

/// A placed prop.
#[derive(Component, Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct Prop {
    /// Catalog key.
    pub prop: String,
    pub settlement: SimId,
    /// The cell and quarter turns it was placed with (the command's values).
    pub x: i32,
    pub z: i32,
    pub rotation: u8,
    /// The cells it covers.
    pub plot: Plot,
}
impl SimHash for Prop {
    const NAME: &'static str = "living_prop_v1";
    fn sim_hash(&self, h: &mut Fnv64) {
        h.str(&self.prop);
        h.u32(self.settlement.0);
        h.i32(self.x);
        h.i32(self.z);
        h.u8(self.rotation);
        for value in [
            self.plot.lo_x,
            self.plot.lo_z,
            self.plot.hi_x,
            self.plot.hi_z,
        ] {
            h.i32(value);
        }
        h.u8(u8::from(self.plot.solid));
    }
}

pub fn register(app: &mut bevy_app::App) {
    app.register_sim_component::<Prop>()
        .register_command::<PlaceProp>()
        .register_command::<RemoveProp>();
}

fn unavailable(reason: String) -> CommandError {
    CommandError::Unavailable { reason }
}

/// The catalog's props, in authored order: the Shape palette.
pub fn catalog(world: &World) -> Vec<PropDef> {
    economy_spec::registry(world).map_or_else(Vec::new, |registry| registry.spec.props.clone())
}

/// A prop described as an open-site building, so the placement survey's
/// ground rules apply: grass or stone (never water, a road or bare soil), at
/// most 1.5 m of slope, nothing else on its cells, near a settlement.
fn as_building(def: &PropDef) -> BuildingDef {
    BuildingDef {
        id: def.id.clone(),
        display: def.display.clone(),
        cost: def.cost.clone(),
        work_ticks: def.work_ticks.max(1),
        footprint: def.footprint,
        kind: BuildingKind::Storage { capacity: 1 },
        solid: def.solid,
        civic: false,
        placement: PlacementRules {
            surfaces: vec![BuildSurface::Grass, BuildSurface::Stone],
            max_slope_mm: 1500,
            clearance: 0,
            coast: false,
        },
        field: None,
        livestock: None,
        upkeep: None,
    }
}

/// The front cell the survey treats as the entrance, for a body whose first
/// row is the cell `(x, z)`.
fn front(x: i32, z: i32, rotation: u8) -> (i32, i32) {
    match rotation % 4 {
        0 => (x, z - 1),
        1 => (x - 1, z),
        2 => (x, z + 1),
        _ => (x + 1, z),
    }
}

/// Places one prop of the catalog with its body starting at cell `(x, z)`
/// and its front facing `rotation` quarter turns from -z. `mode: "build"`
/// opens a construction site with the prop's cost and work (a prop with
/// neither stands at once); `mode: "editor"` places it at once and needs
/// the recorded editor switch ([`super::shape::SetEditorMode`]).
///
/// `{"type": "PlaceProp", "prop": "barrel", "x": 12, "z": -30, "rotation": 0, "mode": "build"}`
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlaceProp {
    pub prop: String,
    pub x: i32,
    pub z: i32,
    #[serde(default)]
    pub rotation: u8,
    pub mode: super::shape::PlaceMode,
}
impl PlaceProp {
    fn plan(&self, world: &World) -> Result<(PropDef, Plot, SimId), String> {
        let def = catalog(world)
            .into_iter()
            .find(|def| def.id == self.prop)
            .ok_or_else(|| format!("`{}` is not a prop of this world's catalog", self.prop))?;
        crate::village::civic::cell_on_map(world, self.x, self.z)?;
        if self.rotation > 3 {
            return Err("rotation must be 0..=3 quarter turns".into());
        }
        let (fx, fz) = front(self.x, self.z, self.rotation);
        let (plot, settlement) =
            crate::village::civic::ground_check(world, &as_building(&def), fx, fz, self.rotation)?;
        if plot.cells().any(|cell| {
            super::terrain::on_road(
                world,
                crate::village::geography::cell_centre(cell.x, cell.z),
            )
        }) {
            return Err(format!(
                "{} cannot stand on a road",
                super::oversee::with_article(&def.display.to_lowercase())
            ));
        }
        Ok((def, plot, settlement))
    }
    /// Whether it is built through a site (build mode, with cost or work).
    fn needs_site(&self, def: &PropDef) -> bool {
        self.mode == super::shape::PlaceMode::Build && (!def.cost.is_empty() || def.work_ticks > 0)
    }
    pub fn check(&self, world: &World) -> Result<(), String> {
        super::shape::editor_gate(world, self.mode)?;
        let (def, _, settlement) = self.plan(world)?;
        if self.needs_site(&def)
            && open_prop_sites(world, settlement) >= crate::village::civic::MAX_OPEN_PROP_SITES
        {
            return Err(
                "Eight small things already wait to be built here. Let the builders catch up first."
                    .into(),
            );
        }
        Ok(())
    }
    /// The new prop, or in build mode the new construction site.
    pub fn perform(&self, world: &mut World) -> Result<SimId, String> {
        self.check(world)?;
        let (def, plot, settlement) = self.plan(world)?;
        let mode = self.mode;
        if self.needs_site(&def) {
            let (fx, fz) = front(self.x, self.z, self.rotation);
            let site = crate::village::civic::place_prop_site(
                world,
                &as_building(&def),
                fx,
                fz,
                self.rotation,
                crate::village::civic::SiteKind::Prop {
                    x: self.x,
                    z: self.z,
                    rotation: self.rotation,
                },
            )?;
            record_effect(
                world,
                None,
                "LivingPropPlaced",
                &json!({"site":site,"kind":def.id,"display":def.display,"x":self.x,"z":self.z,
                    "rotation":self.rotation,"settlement":settlement,"mode":mode}),
            );
            return Ok(site);
        }
        let id = spawn_built(
            world,
            &def.id,
            plot,
            settlement,
            self.x,
            self.z,
            self.rotation,
        );
        record_effect(
            world,
            None,
            "LivingPropPlaced",
            &json!({"prop":id,"kind":def.id,"display":def.display,"x":self.x,"z":self.z,
                "rotation":self.rotation,"solid":plot.solid,"settlement":settlement,"mode":mode}),
        );
        Ok(id)
    }
}

/// Open prop construction sites of a settlement.
pub fn open_prop_sites(world: &World, settlement: SimId) -> usize {
    crate::village::civic::open_sites(world, settlement)
        .into_iter()
        .filter(|(_, entity)| {
            world
                .get::<crate::village::civic::ConstructionSite>(*entity)
                .is_some_and(crate::village::civic::ConstructionSite::is_prop)
        })
        .count()
}

/// Spawns a standing prop on a checked body: a placement in editor mode, a
/// finished prop site, or an exported world's prop.
pub fn spawn_built(
    world: &mut World,
    prop: &str,
    plot: Plot,
    settlement: SimId,
    x: i32,
    z: i32,
    rotation: u8,
) -> SimId {
    let center = SimPos::new(
        (plot.lo_x + plot.hi_x + 1) * 500,
        (plot.lo_z + plot.hi_z + 1) * 500,
    );
    let (id, entity) = spawn_sim(
        world,
        (
            center,
            SettlementMember { settlement },
            Prop {
                prop: prop.to_owned(),
                settlement,
                x,
                z,
                rotation,
                plot,
            },
        ),
    );
    if plot.solid {
        let footprint = Footprint {
            half_x_mm: (plot.hi_x - plot.lo_x + 1) * 500 - 1,
            half_z_mm: (plot.hi_z - plot.lo_z + 1) * 500 - 1,
        };
        world.entity_mut(entity).insert(GridBlocker(footprint));
    }
    id
}

impl ContentCommand for PlaceProp {
    const NAME: &'static str = "PlaceProp";
    fn validate(&self, world: &World) -> Result<(), CommandError> {
        self.check(world).map_err(unavailable)
    }
    fn apply(self, world: &mut World) {
        let _ = self.perform(world);
    }
}

/// Removes a placed prop: the undo of [`PlaceProp`].
///
/// `{"type": "RemoveProp", "prop": 812}`
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoveProp {
    pub prop: SimId,
}
impl RemoveProp {
    pub fn check(&self, world: &World) -> Result<(), String> {
        let entity = world.resource::<SimIndex>().get(self.prop);
        let standing = entity.is_some_and(|entity| world.get::<Prop>(entity).is_some());
        let site = entity.is_some_and(|entity| {
            world
                .get::<crate::village::civic::ConstructionSite>(entity)
                .is_some_and(crate::village::civic::ConstructionSite::is_prop)
        });
        if standing || site {
            Ok(())
        } else {
            Err("that prop no longer exists".into())
        }
    }
    pub fn perform(&self, world: &mut World) -> Result<(), String> {
        self.check(world)?;
        // A prop still being built: its site is cancelled (delivered goods go
        // back as for `CancelBuilding`).
        if world
            .resource::<SimIndex>()
            .get(self.prop)
            .is_some_and(|entity| world.get::<Prop>(entity).is_none())
        {
            crate::village::civic::cancel_site(world, self.prop, "removed by the player");
            return Ok(());
        }
        let entity = world
            .resource::<SimIndex>()
            .get(self.prop)
            .expect("checked");
        let prop = world.get::<Prop>(entity).cloned().expect("checked");
        world.despawn(entity);
        record_effect(
            world,
            None,
            "LivingPropRemoved",
            &json!({"prop":self.prop,"kind":prop.prop,"x":prop.x,"z":prop.z,
                "rotation":prop.rotation,"settlement":prop.settlement}),
        );
        Ok(())
    }
}
impl ContentCommand for RemoveProp {
    const NAME: &'static str = "RemoveProp";
    fn validate(&self, world: &World) -> Result<(), CommandError> {
        self.check(world).map_err(unavailable)
    }
    fn apply(self, world: &mut World) {
        let _ = self.perform(world);
    }
}

/// One placed prop, for drawing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PropView {
    pub id: SimId,
    /// Catalog key.
    pub prop: String,
    pub display: String,
    /// Model path from the catalog; empty: draw the generic marker.
    pub model: String,
    /// The cell and quarter turns it was placed with.
    pub x: i32,
    pub z: i32,
    pub rotation: u8,
    /// Inclusive body cells.
    pub body_min: (i32, i32),
    pub body_max: (i32, i32),
    pub solid: bool,
    pub settlement: SimId,
}

/// Every placed prop, ascending by identity.
pub fn props(world: &World) -> Vec<PropView> {
    let defs = catalog(world);
    let Some(mut query) = world.try_query::<(&SimId, &Prop)>() else {
        return Vec::new();
    };
    let mut out: Vec<PropView> = query
        .iter(world)
        .map(|(id, prop)| {
            let def = defs.iter().find(|def| def.id == prop.prop);
            PropView {
                id: *id,
                prop: prop.prop.clone(),
                display: def.map_or_else(|| prop.prop.clone(), |def| def.display.clone()),
                model: def.map_or_else(String::new, |def| def.model.clone()),
                x: prop.x,
                z: prop.z,
                rotation: prop.rotation,
                body_min: (prop.plot.lo_x, prop.plot.lo_z),
                body_max: (prop.plot.hi_x, prop.plot.hi_z),
                solid: prop.plot.solid,
                settlement: prop.settlement,
            }
        })
        .collect();
    out.sort_unstable_by_key(|view| view.id);
    out
}

/// The props of an export, ascending by identity: standing props, and
/// props still being built (laid out standing when the world is built).
pub fn export(world: &World) -> Vec<PlacedProp> {
    let mut out: Vec<(SimId, PlacedProp)> = props(world)
        .into_iter()
        .map(|view| {
            (
                view.id,
                PlacedProp {
                    prop: view.prop,
                    x: view.x,
                    z: view.z,
                    rotation: view.rotation,
                },
            )
        })
        .collect();
    if let Some(mut query) = world.try_query::<(&SimId, &crate::village::civic::ConstructionSite)>()
    {
        for (id, site) in query.iter(world) {
            if let crate::village::civic::SiteKind::Prop { x, z, rotation } = site.kind {
                out.push((
                    *id,
                    PlacedProp {
                        prop: site.building.clone(),
                        x,
                        z,
                        rotation,
                    },
                ));
            }
        }
    }
    out.sort_unstable_by_key(|(id, _)| *id);
    out.into_iter().map(|(_, prop)| prop).collect()
}

/// Lays out the props of an exported world.
pub fn spawn(world: &mut World) {
    let placed = super::spec(world)
        .map(|spec| spec.props.clone())
        .unwrap_or_default();
    for prop in placed {
        let command = PlaceProp {
            prop: prop.prop.clone(),
            x: prop.x,
            z: prop.z,
            rotation: prop.rotation,
            mode: super::shape::PlaceMode::Editor,
        };
        // An exported world's props stand as they were: no editor switch.
        let laid = command.plan(world).map(|(def, plot, settlement)| {
            spawn_built(
                world,
                &def.id,
                plot,
                settlement,
                prop.x,
                prop.z,
                prop.rotation,
            )
        });
        if let Err(reason) = laid {
            record_effect(
                world,
                None,
                "RegionalPropNotLaidOut",
                &json!({"prop":prop.prop,"x":prop.x,"z":prop.z,"reason":reason}),
            );
        }
    }
}
