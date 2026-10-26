use crate::{MAX_INVENTORY_ITEMS, MAX_ITEM_COUNT, SandboxError, finite_range, valid_id};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub type Inventory = BTreeMap<String, u32>;

/// Replace or extend through JSON, without adding code to the simulation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Catalog {
    pub items: BTreeSet<String>,
    pub recipes: BTreeMap<String, Recipe>,
    pub blueprints: BTreeMap<String, Blueprint>,
    pub crops: BTreeMap<String, CropDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recipe {
    pub inputs: Inventory,
    pub outputs: Inventory,
    pub requires_workbench: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Blueprint {
    pub costs: Inventory,
    pub hitpoints: u32,
    pub radius_m: f64,
    pub housing: bool,
    pub workbench: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CropDefinition {
    pub seed_item: String,
    pub harvest_item: String,
    pub harvest_amount: u32,
    pub growth_seconds: f64,
}

impl Catalog {
    pub fn builtin() -> Self {
        let items = [
            "wood",
            "stone",
            "planks",
            "stone_blocks",
            "tool",
            "wheat_seed",
            "wheat",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        let recipes = BTreeMap::from([
            (
                "planks".into(),
                Recipe {
                    inputs: inventory(&[("wood", 2)]),
                    outputs: inventory(&[("planks", 4)]),
                    requires_workbench: false,
                },
            ),
            (
                "stone_blocks".into(),
                Recipe {
                    inputs: inventory(&[("stone", 2)]),
                    outputs: inventory(&[("stone_blocks", 1)]),
                    requires_workbench: false,
                },
            ),
            (
                "tool".into(),
                Recipe {
                    inputs: inventory(&[("planks", 2), ("stone", 2)]),
                    outputs: inventory(&[("tool", 1)]),
                    requires_workbench: true,
                },
            ),
            (
                "wheat_seed".into(),
                Recipe {
                    inputs: inventory(&[("wheat", 1)]),
                    outputs: inventory(&[("wheat_seed", 3)]),
                    requires_workbench: false,
                },
            ),
        ]);
        let blueprints = BTreeMap::from([
            (
                "house".into(),
                Blueprint {
                    costs: inventory(&[("planks", 12), ("stone_blocks", 4)]),
                    hitpoints: 200,
                    radius_m: 4.0,
                    housing: true,
                    workbench: false,
                },
            ),
            (
                "workbench".into(),
                Blueprint {
                    costs: inventory(&[("planks", 4), ("stone", 2)]),
                    hitpoints: 80,
                    radius_m: 1.0,
                    housing: false,
                    workbench: true,
                },
            ),
            (
                "fence".into(),
                Blueprint {
                    costs: inventory(&[("planks", 2)]),
                    hitpoints: 40,
                    radius_m: 0.7,
                    housing: false,
                    workbench: false,
                },
            ),
        ]);
        let crops = BTreeMap::from([(
            "wheat".into(),
            CropDefinition {
                seed_item: "wheat_seed".into(),
                harvest_item: "wheat".into(),
                harvest_amount: 4,
                growth_seconds: 180.0,
            },
        )]);
        Self {
            items,
            recipes,
            blueprints,
            crops,
        }
    }

    pub fn from_json(json: &str) -> Result<Self, SandboxError> {
        if json.len() > 1024 * 1024 {
            return Err(SandboxError::Capacity);
        }
        let catalog: Self = serde_json::from_str(json)?;
        catalog.validate()?;
        Ok(catalog)
    }

    pub fn validate(&self) -> Result<(), SandboxError> {
        if self.items.len() > MAX_INVENTORY_ITEMS
            || self.recipes.len() > 256
            || self.blueprints.len() > 128
            || self.crops.len() > 64
        {
            return Err(SandboxError::Capacity);
        }
        if self.items.iter().any(|id| !valid_id(id)) {
            return Err(SandboxError::Invalid("item identifier".into()));
        }
        for (id, recipe) in &self.recipes {
            if !valid_id(id) || recipe.inputs.is_empty() || recipe.outputs.is_empty() {
                return Err(SandboxError::Invalid("recipe".into()));
            }
            self.validate_items(&recipe.inputs)?;
            self.validate_items(&recipe.outputs)?;
        }
        for (id, blueprint) in &self.blueprints {
            if !valid_id(id)
                || blueprint.hitpoints == 0
                || blueprint.hitpoints > MAX_ITEM_COUNT
                || !finite_range(blueprint.radius_m, 0.1, 50.0)
            {
                return Err(SandboxError::Invalid("blueprint".into()));
            }
            self.validate_items(&blueprint.costs)?;
        }
        for (id, crop) in &self.crops {
            if !valid_id(id)
                || !self.items.contains(&crop.seed_item)
                || !self.items.contains(&crop.harvest_item)
                || crop.harvest_amount == 0
                || crop.harvest_amount > 10_000
                || !finite_range(crop.growth_seconds, 1.0, 31_536_000.0)
            {
                return Err(SandboxError::Invalid("crop definition".into()));
            }
        }
        Ok(())
    }

    fn validate_items(&self, items: &Inventory) -> Result<(), SandboxError> {
        validate_inventory(items)?;
        if items
            .iter()
            .any(|(id, count)| !self.items.contains(id) || *count == 0)
        {
            return Err(SandboxError::Invalid("catalog item reference".into()));
        }
        Ok(())
    }
}

impl Default for Catalog {
    fn default() -> Self {
        Self::builtin()
    }
}

pub(crate) fn inventory(items: &[(&str, u32)]) -> Inventory {
    items
        .iter()
        .map(|(id, count)| ((*id).into(), *count))
        .collect()
}

pub(crate) fn validate_inventory(value: &Inventory) -> Result<(), SandboxError> {
    if value.len() > MAX_INVENTORY_ITEMS {
        return Err(SandboxError::Capacity);
    }
    if value
        .iter()
        .any(|(id, count)| !valid_id(id) || *count > MAX_ITEM_COUNT)
    {
        return Err(SandboxError::Invalid("inventory".into()));
    }
    Ok(())
}

/// Plan complete debit+credit on a small bounded inventory. The caller commits
/// this value only after every other command precondition has succeeded.
pub(crate) fn exchange(
    current: &Inventory,
    costs: &Inventory,
    rewards: &Inventory,
    multiplier: u32,
) -> Result<Inventory, SandboxError> {
    let mut next = current.clone();
    for (item, amount) in costs {
        let cost = amount
            .checked_mul(multiplier)
            .ok_or(SandboxError::Capacity)?;
        let have = next.get(item).copied().unwrap_or(0);
        if have < cost {
            return Err(SandboxError::Insufficient(item.clone()));
        }
        if have == cost {
            next.remove(item);
        } else {
            next.insert(item.clone(), have - cost);
        }
    }
    for (item, amount) in rewards {
        let reward = amount
            .checked_mul(multiplier)
            .ok_or(SandboxError::Capacity)?;
        let count = next
            .get(item)
            .copied()
            .unwrap_or(0)
            .checked_add(reward)
            .ok_or(SandboxError::Capacity)?;
        if count > MAX_ITEM_COUNT {
            return Err(SandboxError::Capacity);
        }
        next.insert(item.clone(), count);
    }
    validate_inventory(&next)?;
    Ok(next)
}
