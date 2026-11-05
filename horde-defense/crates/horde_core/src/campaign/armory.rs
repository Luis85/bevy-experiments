//! Owned items, presets, equipment reservations and weapon access.

use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    /// None preserves the authored base quality and all historical numbers.
    #[serde(default)]
    pub quality: Option<crate::quality::Quality>,
    #[serde(default = "crate::progression::level_one")]
    pub level: u32,
    pub id: u64,
    pub definition: ItemDefId,
    pub locked: bool,
    pub favorite: bool,
    pub protected: bool,
}

impl Item {
    /// Authored definition; saved IDs are validated against the catalogue on load.
    pub fn definition(&self) -> &'static ItemDefinition {
        self.definition
            .definition()
            .expect("validated item definition ID")
    }
    /// Position of this item's definition in the loaded catalogue.
    pub fn definition_index(&self) -> usize {
        self.definition
            .index()
            .expect("validated item definition ID")
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preset {
    pub name: String,
    pub loadout: [u64; 4],
}

impl Campaign {
    pub fn can_use_weapon(&self, mode: Mode, definition: usize) -> bool {
        self.weapon_access(mode, definition).is_ok()
    }
    pub fn buy_price(&self, definition: usize) -> Result<u64, String> {
        Ok(crate::progression::price(
            content()
                .items
                .get(definition)
                .ok_or("Unknown equipment")?
                .buy_value,
            self.shop_item_level(),
        ))
    }
    pub fn item_sale_value(&self, id: u64) -> Result<u64, String> {
        let item = self.item(id).ok_or("Unknown item")?;
        Ok(
            (crate::progression::price(item.definition().sell_value, item.level) as f64
                * crate::quality::value_ratio(item.definition().base_quality, item.quality))
            .round() as u64,
        )
    }
    pub fn item_quality(&self, id: u64) -> Result<crate::quality::Quality, String> {
        let item = self.item(id).ok_or("Unknown item")?;
        Ok(item.quality.unwrap_or(item.definition().base_quality))
    }
    pub fn effective_item(&self, id: u64) -> Result<crate::content::ItemDefinition, String> {
        let item = self.item(id).ok_or("Unknown item")?;
        Ok(crate::quality::apply(
            crate::progression::scale_item(item.definition().clone(), item.level),
            item.quality,
        ))
    }
    /// Shared access reason for equipment actions, deployment and presentation.
    pub fn weapon_access(&self, mode: Mode, definition: usize) -> Result<(), &'static str> {
        let Some(item) = content().items.get(definition) else {
            return Err("Unknown equipment");
        };
        // NPCs retain their independent Guns skill/training and attack procedure.
        if mode == Mode::Npc {
            return Ok(());
        }
        if item.npc_only && !self.legacy_weapon_training {
            return Err(
                "NPC issued gear is reserved for NPC defenders. Buy or loot your own weapon; proficiency does not grant equipment.",
            );
        }
        if item.slot == Slot::Ranged
            && !self
                .modifiers()
                .weapon_classes
                .contains(&WeaponSpec::from_item(item).class)
        {
            return Err(
                "Learn this weapon class proficiency in Skills before equipping this owned weapon.",
            );
        }
        Ok(())
    }
    pub fn item(&self, id: u64) -> Option<&Item> {
        self.items
            .binary_search_by_key(&id, |item| item.id)
            .ok()
            .map(|index| &self.items[index])
    }
    pub fn equipped(&self, mode: Mode, slot: Slot) -> &crate::content::ItemDefinition {
        let loadout = match mode {
            Mode::Personal => self.player.loadout,
            Mode::Npc => self.npc.loadout,
        };
        self.item(loadout[slot.index()])
            .map_or(&content().items[slot.index()], Item::definition)
    }
    pub fn add_item(&mut self, definition: usize) -> Result<u64, String> {
        self.add_item_at_level(definition, 1)
    }
    pub fn add_item_at_level(&mut self, definition: usize, level: u32) -> Result<u64, String> {
        self.add_item_with_quality(definition, level, None)
    }
    pub fn add_item_with_quality(
        &mut self,
        definition: usize,
        level: u32,
        quality: Option<crate::quality::Quality>,
    ) -> Result<u64, String> {
        if !(1..=64).contains(&level) {
            return Err("Unsupported item level".into());
        }
        if definition >= content().items.len() {
            return Err("Unknown item definition".into());
        }
        let id = self.next_item_id;
        let next = id
            .checked_add(1)
            .filter(|_| id > 0)
            .ok_or("Persistent item identity space exhausted")?;
        self.next_item_id = next;
        self.items.push(Item {
            quality,
            level,
            id,
            definition: ItemDefId::at(definition),
            locked: false,
            favorite: false,
            protected: false,
        });
        self.kills_since_loot = 0;
        Ok(id)
    }
    pub fn equip(&mut self, mode: Mode, id: u64) -> Result<(), String> {
        self.preparation()?;
        if self.reserved_items.contains(&id) {
            return Err("This equipment is reserved by another survivor".into());
        }
        let item = self.item(id).ok_or("Item is no longer owned")?;
        let slot = item.definition().slot.index();
        self.weapon_access(mode, item.definition_index())
            .map_err(str::to_owned)?;
        match mode {
            Mode::Personal => self.player.loadout[slot] = id,
            Mode::Npc => self.npc.loadout[slot] = id,
        };
        Ok(())
    }
    pub fn set_lock(&mut self, id: u64, locked: bool) -> Result<(), String> {
        self.preparation()?;
        let item = self
            .items
            .iter_mut()
            .find(|x| x.id == id)
            .ok_or("Unknown item")?;
        if item.protected && !locked {
            return Err("Starter equipment remains protected".into());
        }
        item.locked = locked;
        Ok(())
    }
    pub fn set_favorite(&mut self, id: u64, favorite: bool) -> Result<(), String> {
        self.preparation()?;
        let item = self
            .items
            .iter_mut()
            .find(|x| x.id == id)
            .ok_or("Unknown item")?;
        item.favorite = favorite;
        Ok(())
    }
    pub fn save_preset(&mut self, name: String, mode: Mode) -> Result<(), String> {
        self.preparation()?;
        if name.trim().is_empty() || name.len() > 48 {
            return Err("Preset name must contain 1-48 bytes".into());
        }
        let loadout = if mode == Mode::Personal {
            self.player.loadout
        } else {
            self.npc.loadout
        };
        if let Some(p) = self.presets.iter_mut().find(|x| x.name == name) {
            p.loadout = loadout;
        } else {
            self.presets.push(Preset { name, loadout });
        }
        Ok(())
    }
    pub fn apply_preset(&mut self, index: usize, mode: Mode) -> Result<(), String> {
        self.preparation()?;
        let loadout = self.presets.get(index).ok_or("Unknown preset")?.loadout;
        self.validate_loadout(loadout)?;
        for id in loadout {
            let item = self.item(id).ok_or("Missing equipment")?;
            self.weapon_access(mode, item.definition_index())
                .map_err(str::to_owned)?;
        }
        if loadout.iter().any(|id| self.reserved_items.contains(id)) {
            return Err("Preset equipment is reserved by another survivor".into());
        }
        match mode {
            Mode::Personal => self.player.loadout = loadout,
            Mode::Npc => self.npc.loadout = loadout,
        };
        Ok(())
    }
    pub(super) fn personal_starter_id(&self, slot: usize) -> u64 {
        if slot == 0
            && self.items.iter().any(|i| {
                i.id == 5
                    && i.protected
                    && WeaponSpec::from_item(i.definition()).class == WeaponClass::Pistol
            })
        {
            5
        } else {
            slot as u64 + 1
        }
    }
    pub fn validate_loadout(&self, loadout: [u64; 4]) -> Result<(), String> {
        for (slot, id) in loadout.into_iter().enumerate() {
            let item = self.item(id).ok_or("Preset references sold equipment")?;
            if item
                .definition
                .definition()
                .ok_or("Unsupported equipment")?
                .slot
                .index()
                != slot
            {
                return Err("Equipment assigned to an incompatible slot".into());
            }
        }
        Ok(())
    }
    pub fn reserve(&mut self, mode: Mode) -> Result<(), String> {
        self.preparation()?;
        if mode == Mode::Npc && !self.npc_available() {
            return Err(if self.legacy_features {
                "Complete your introductory attempt to meet Mara".into()
            } else {
                "Learn the companion path after liberating a country".into()
            });
        }
        self.validate_loadout(if mode == Mode::Personal {
            self.player.loadout
        } else {
            self.npc.loadout
        })?;
        let loadout = if mode == Mode::Personal {
            self.player.loadout
        } else {
            self.npc.loadout
        };
        if loadout.iter().any(|id| self.reserved_items.contains(id)) {
            return Err("Equipment is assigned to another deployed survivor or guard".into());
        }
        for id in loadout {
            let item = self.item(id).ok_or("Missing equipment")?;
            self.weapon_access(mode, item.definition_index())
                .map_err(str::to_owned)?;
        }
        self.reserved = Some(mode);
        Ok(())
    }
    pub fn grant_recruit_starters(&mut self) -> Result<[u64; 4], String> {
        self.next_item_id
            .checked_add(4)
            .filter(|_| self.next_item_id > 0)
            .ok_or("Persistent item identity space exhausted")?;
        let mut loadout = [0; 4];
        for (n, id) in loadout.iter_mut().enumerate() {
            *id = self.add_item(n)?;
            let item = self.items.last_mut().expect("just added");
            item.locked = true;
            item.protected = true;
        }
        Ok(loadout)
    }
    /// The mode owner's loadout the main run holds. A recruit takeover
    /// (`reserved_recruit` Some) reserves the recruit's items through
    /// `reserved_items` instead, so Mara's idle loadout stays free.
    fn main_run_loadout(&self) -> Option<[u64; 4]> {
        match self.reserved {
            Some(Mode::Personal) => Some(self.player.loadout),
            Some(Mode::Npc) if self.reserved_recruit.is_none() => Some(self.npc.loadout),
            Some(Mode::Npc) | None => None,
        }
    }
    pub fn item_reserved(&self, id: u64) -> bool {
        self.reserved_items.contains(&id)
            || self
                .main_run_loadout()
                .is_some_and(|loadout| loadout.contains(&id))
    }
    pub fn reserve_external(&mut self, loadout: [u64; 4]) -> Result<(), String> {
        self.validate_loadout(loadout)?;
        let main = self.main_run_loadout();
        if loadout.iter().any(|id| {
            self.reserved_items.contains(id) || main.is_some_and(|items| items.contains(id))
        }) {
            return Err("A unique item is already reserved by another deployment".into());
        }
        self.reserved_items.extend(loadout);
        Ok(())
    }
    pub fn release_external(&mut self, loadout: [u64; 4]) {
        self.reserved_items.retain(|id| !loadout.contains(id));
    }
}
