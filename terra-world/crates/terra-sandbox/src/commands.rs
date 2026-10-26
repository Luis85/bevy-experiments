use crate::catalog::{exchange, inventory};
use crate::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use terra_core::GeoPosition;

/// Requests contain intent only; actor identity and position are supplied by
/// trusted server state. Commands never accept inventory, hitpoints or yields.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Living {
        action: LivingCommand,
    },
    ClaimPlot {
        center: GeoPosition,
        radius_m: f64,
    },
    Build {
        blueprint: String,
        position: GeoPosition,
    },
    Damage {
        structure_id: u64,
        amount: u32,
    },
    Gather {
        node_id: String,
        amount: u32,
    },
    Craft {
        recipe: String,
        count: u32,
    },
    Plant {
        plot_id: u64,
        position: GeoPosition,
        crop: String,
    },
    Harvest {
        farm_id: u64,
    },
    SetHome {
        structure_id: u64,
    },
    RemoveFeature {
        feature_id: String,
    },
    CreateFaction {
        name: String,
    },
    InviteToFaction {
        faction_id: u64,
        player_id: u64,
    },
    AcceptFactionInvite {
        faction_id: u64,
    },
    SetFactionRole {
        faction_id: u64,
        player_id: u64,
        role: FactionRole,
    },
    TransferPlotToFaction {
        plot_id: u64,
        faction_id: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Outcome {
    pub message: String,
    pub created_id: Option<u64>,
}

impl Outcome {
    fn message(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            created_id: None,
        }
    }
    fn created(message: impl Into<String>, id: u64) -> Self {
        Self {
            message: message.into(),
            created_id: Some(id),
        }
    }
}

impl SandboxState {
    /// Validate the complete command before mutation. Only the affected bounded
    /// inventory is copied; the world is not cloned for every player action.
    ///
    /// Catalogs are configuration, not client input. Validate a catalog once at
    /// startup. Hosting servers additionally enforce command rate limits.
    pub fn apply(
        &mut self,
        player_id: u64,
        command: Command,
        catalog: &Catalog,
    ) -> Result<Outcome, SandboxError> {
        // Reject oversized/untrusted identifiers before cloning them into an
        // error or response. Catalog/state identifiers use the same bound.
        let identifier = match &command {
            Command::Build { blueprint, .. } => Some(blueprint),
            Command::Gather { node_id, .. } => Some(node_id),
            Command::Craft { recipe, .. } => Some(recipe),
            Command::Plant { crop, .. } => Some(crop),
            Command::RemoveFeature { feature_id } => Some(feature_id),
            _ => None,
        };
        if identifier.is_some_and(|id| !valid_id(id)) {
            return Err(SandboxError::Invalid("command identifier".into()));
        }
        let player = self
            .players
            .get(&player_id)
            .ok_or(SandboxError::PlayerMissing)?;
        let player_position = player.position;
        let require_reach = |position: GeoPosition| -> Result<(), SandboxError> {
            position.validate()?;
            // Vertical interaction reach matters for aircraft/rooftop actors.
            let horizontal = player_position.surface_distance_m(position);
            let vertical = player_position.altitude_m - position.altitude_m;
            if horizontal.hypot(vertical) > INTERACTION_REACH_M {
                return Err(SandboxError::OutOfReach);
            }
            Ok(())
        };
        match command {
            Command::Living { action } => self.apply_living(player_id, action, catalog),
            Command::ClaimPlot { center, radius_m } => {
                require_reach(center)?;
                if !finite_range(radius_m, 5.0, 100.0) {
                    return Err(SandboxError::Invalid(
                        "claim radius must be 5..100 metres".into(),
                    ));
                }
                if self.plots.len() >= MAX_ENTITIES {
                    return Err(SandboxError::Capacity);
                }
                if self
                    .plots
                    .values()
                    .any(|p| p.center.surface_distance_m(center) < p.radius_m + radius_m)
                {
                    return Err(SandboxError::Overlap);
                }
                let id = self.fresh_id()?;
                self.plots.insert(
                    id,
                    LandPlot {
                        id,
                        owner: player_id,
                        faction_owner: None,
                        center,
                        radius_m,
                    },
                );
                self.next_id += 1;
                Ok(Outcome::created("Land claimed", id))
            }
            Command::Build {
                blueprint,
                position,
            } => {
                require_reach(position)?;
                let definition = catalog
                    .blueprints
                    .get(&blueprint)
                    .ok_or_else(|| SandboxError::Missing(blueprint.clone()))?;
                if self.structures.len() >= MAX_ENTITIES {
                    return Err(SandboxError::Capacity);
                }
                let faction_owner = self
                    .plot_for_area(player_id, position, definition.radius_m)
                    .ok_or(SandboxError::NotOwner)?
                    .faction_owner;
                if self.structures.values().any(|s| {
                    s.position.surface_distance_m(position) < s.radius_m + definition.radius_m
                }) || self.farms.values().any(|farm| {
                    farm.position.surface_distance_m(position) < 0.5 + definition.radius_m
                }) {
                    return Err(SandboxError::Overlap);
                }
                let height = if definition.housing { 8.0 } else { 2.5 };
                if self.living.enabled
                    && self.living.units.values().any(|unit| {
                        let vertical = unit.position.altitude_m - position.altitude_m;
                        let body_radius = if matches!(
                            unit.kind,
                            terra_core::ActorKind::Vehicle | terra_core::ActorKind::Ship
                        ) {
                            1.2
                        } else {
                            0.35
                        };
                        (-2.0..=height).contains(&vertical)
                            && unit.position.surface_distance_m(position)
                                < definition.radius_m + body_radius
                    })
                {
                    return Err(SandboxError::Invalid(
                        "construction footprint overlaps a living actor".into(),
                    ));
                }
                let next_inventory =
                    exchange(&player.inventory, &definition.costs, &BTreeMap::new(), 1)?;
                let id = self.fresh_id()?;
                let structure = Structure {
                    id,
                    owner: player_id,
                    faction_owner,
                    blueprint,
                    position,
                    hitpoints: definition.hitpoints,
                    max_hitpoints: definition.hitpoints,
                    radius_m: definition.radius_m,
                    housing: definition.housing,
                    workbench: definition.workbench,
                };
                self.players
                    .get_mut(&player_id)
                    .expect("player validated")
                    .inventory = next_inventory;
                self.structures.insert(id, structure);
                self.next_id += 1;
                Ok(Outcome::created("Structure built", id))
            }
            Command::Damage {
                structure_id,
                amount,
            } => {
                if amount == 0 || amount > 25 {
                    return Err(SandboxError::Invalid(
                        "damage must be 1..25 per action".into(),
                    ));
                }
                let structure = self
                    .structures
                    .get(&structure_id)
                    .ok_or_else(|| SandboxError::Missing(format!("structure:{structure_id}")))?;
                if !self.may_interact(player_id, structure.faction_owner, structure.owner) {
                    return Err(SandboxError::NotOwner);
                }
                require_reach(structure.position)?;
                if amount >= structure.hitpoints {
                    self.structures.remove(&structure_id);
                    for player in self.players.values_mut() {
                        if player.home == Some(structure_id) {
                            player.home = None;
                        }
                    }
                    // No refunds: demolition cannot be used to duplicate items.
                    Ok(Outcome::message("Structure destroyed"))
                } else {
                    self.structures
                        .get_mut(&structure_id)
                        .expect("structure validated")
                        .hitpoints -= amount;
                    Ok(Outcome::message("Structure damaged"))
                }
            }
            Command::Gather { node_id, amount } => {
                if amount == 0 || amount > 100 {
                    return Err(SandboxError::Invalid("gather amount must be 1..100".into()));
                }
                let node = self
                    .resource_nodes
                    .get(&node_id)
                    .ok_or_else(|| SandboxError::Missing(node_id.clone()))?;
                require_reach(node.position)?;
                if self.plots.values().any(|plot| {
                    !self.may_interact(player_id, plot.faction_owner, plot.owner)
                        && plot.center.surface_distance_m(node.position) <= plot.radius_m
                }) {
                    return Err(SandboxError::NotOwner);
                }
                if node.remaining < amount {
                    return Err(SandboxError::Insufficient(node.item.clone()));
                }
                if !catalog.items.contains(&node.item) {
                    return Err(SandboxError::Missing(node.item.clone()));
                }
                let next_inventory = exchange(
                    &player.inventory,
                    &BTreeMap::new(),
                    &inventory(&[(&node.item, amount)]),
                    1,
                )?;
                self.players
                    .get_mut(&player_id)
                    .expect("player validated")
                    .inventory = next_inventory;
                self.resource_nodes
                    .get_mut(&node_id)
                    .expect("node validated")
                    .remaining -= amount;
                Ok(Outcome::message(format!("Gathered {amount} items")))
            }
            Command::Craft { recipe, count } => {
                if count == 0 || count > 20 {
                    return Err(SandboxError::Invalid("craft count must be 1..20".into()));
                }
                let definition = catalog
                    .recipes
                    .get(&recipe)
                    .ok_or_else(|| SandboxError::Missing(recipe.clone()))?;
                if definition.requires_workbench
                    && !self.structures.values().any(|s| {
                        self.may_use(player_id, s.faction_owner, s.owner)
                            && s.workbench
                            && player_position
                                .surface_distance_m(s.position)
                                .hypot(player_position.altitude_m - s.position.altitude_m)
                                <= INTERACTION_REACH_M
                    })
                {
                    return Err(SandboxError::WorkbenchRequired);
                }
                let next_inventory = exchange(
                    &player.inventory,
                    &definition.inputs,
                    &definition.outputs,
                    count,
                )?;
                self.players
                    .get_mut(&player_id)
                    .expect("player validated")
                    .inventory = next_inventory;
                Ok(Outcome::message(format!("Crafted {recipe} x{count}")))
            }
            Command::Plant {
                plot_id,
                position,
                crop,
            } => {
                require_reach(position)?;
                let plot = self
                    .plots
                    .get(&plot_id)
                    .ok_or_else(|| SandboxError::Missing(format!("plot:{plot_id}")))?;
                if !self.may_build(player_id, plot.faction_owner, plot.owner)
                    || plot.center.surface_distance_m(position) + 0.5 > plot.radius_m
                {
                    return Err(SandboxError::NotOwner);
                }
                let definition = catalog
                    .crops
                    .get(&crop)
                    .ok_or_else(|| SandboxError::Missing(crop.clone()))?;
                if self.farms.len() >= MAX_ENTITIES {
                    return Err(SandboxError::Capacity);
                }
                if self
                    .farms
                    .values()
                    .any(|farm| farm.position.surface_distance_m(position) < 1.0)
                    || self
                        .structures
                        .values()
                        .any(|s| s.position.surface_distance_m(position) < s.radius_m + 0.5)
                {
                    return Err(SandboxError::Overlap);
                }
                let next_inventory = exchange(
                    &player.inventory,
                    &inventory(&[(&definition.seed_item, 1)]),
                    &BTreeMap::new(),
                    1,
                )?;
                let id = self.fresh_id()?;
                let farm = FarmPlot {
                    id,
                    owner: player_id,
                    faction_owner: plot.faction_owner,
                    plot_id,
                    position,
                    crop,
                    growth_seconds: 0.0,
                    required_growth_seconds: definition.growth_seconds,
                    moisture: 0.8,
                    harvest_item: definition.harvest_item.clone(),
                    harvest_amount: definition.harvest_amount,
                };
                self.players
                    .get_mut(&player_id)
                    .expect("player validated")
                    .inventory = next_inventory;
                self.farms.insert(id, farm);
                self.next_id += 1;
                Ok(Outcome::created("Crop planted", id))
            }
            Command::Harvest { farm_id } => {
                let farm = self
                    .farms
                    .get(&farm_id)
                    .ok_or_else(|| SandboxError::Missing(format!("farm:{farm_id}")))?;
                if !self.may_interact(player_id, farm.faction_owner, farm.owner) {
                    return Err(SandboxError::NotOwner);
                }
                require_reach(farm.position)?;
                if !farm.ready() {
                    return Err(SandboxError::NotReady);
                }
                let next_inventory = exchange(
                    &player.inventory,
                    &BTreeMap::new(),
                    &inventory(&[(&farm.harvest_item, farm.harvest_amount)]),
                    1,
                )?;
                self.players
                    .get_mut(&player_id)
                    .expect("player validated")
                    .inventory = next_inventory;
                self.farms.remove(&farm_id);
                Ok(Outcome::message("Crop harvested"))
            }
            Command::SetHome { structure_id } => {
                let structure = self
                    .structures
                    .get(&structure_id)
                    .ok_or_else(|| SandboxError::Missing(format!("structure:{structure_id}")))?;
                if !self.may_use(player_id, structure.faction_owner, structure.owner) {
                    return Err(SandboxError::NotOwner);
                }
                if !structure.housing {
                    return Err(SandboxError::Invalid("structure is not housing".into()));
                }
                require_reach(structure.position)?;
                self.players
                    .get_mut(&player_id)
                    .expect("player validated")
                    .home = Some(structure_id);
                Ok(Outcome::message("Home set"))
            }
            Command::RemoveFeature { feature_id } => {
                let feature = self
                    .world_features
                    .get(&feature_id)
                    .ok_or_else(|| SandboxError::Missing(feature_id.clone()))?;
                // Source ways may be clipped across chunks. Owning a visible
                // fragment cannot establish ownership of the complete upstream
                // geometry, so 0.1 edits only complete point features (trees).
                if feature.kind != terra_core::FeatureKind::Tree {
                    return Err(SandboxError::Invalid(
                        "source geometry is immutable; only tree point features can be removed"
                            .into(),
                    ));
                }
                require_reach(feature.position)?;
                if !self.owns_area(player_id, feature.position, feature.radius_m) {
                    return Err(SandboxError::NotOwner);
                }
                if self.feature_edits.contains(&feature_id) {
                    return Err(SandboxError::Invalid("feature already removed".into()));
                }
                if self.feature_edits.len() >= MAX_ENTITIES {
                    return Err(SandboxError::Capacity);
                }
                self.feature_edits.insert(feature_id.clone());
                self.resource_nodes
                    .retain(|_, node| node.source_feature.as_ref() != Some(&feature_id));
                Ok(Outcome::message("World feature removed in player overlay"))
            }
            Command::CreateFaction { name } => self.create_faction(player_id, name),
            Command::InviteToFaction {
                faction_id,
                player_id: invitee,
            } => self.invite_faction(player_id, faction_id, invitee),
            Command::AcceptFactionInvite { faction_id } => {
                self.accept_faction(player_id, faction_id)
            }
            Command::SetFactionRole {
                faction_id,
                player_id: target,
                role,
            } => self.set_faction_role(player_id, faction_id, target, role),
            Command::TransferPlotToFaction {
                plot_id,
                faction_id,
            } => self.transfer_plot_to_faction(player_id, plot_id, faction_id),
        }
    }
}
