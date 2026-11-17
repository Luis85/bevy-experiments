//! The way out of a blocked sale (LE-1, W3-21). [`Game::sale_resolution`]
//! names the shortcut the armory offers beside `SaleBlock::label()`, and
//! [`Game::sale_resolution_command`] the command that shortcut dispatches.
//! Both are pure reads; the block itself is still decided only by
//! `Campaign::sale_block`.
use super::*;
use crate::campaign::SaleResolution;
use crate::command::Command;
use crate::content::Slot;

impl Game {
    /// The shortcut out of the guard that blocks selling `id` now
    /// ([`Game::sale_eligibility`]), or `None` when the item is sellable or
    /// its guard has no way out (unknown, in battle, reserved, protected or
    /// NPC-issue gear). A preset block names the first preset that holds the
    /// item, a recruit block the first recruit who wears it. Pure: no
    /// mutation, no RNG.
    pub fn sale_resolution(&self, id: u64) -> SaleResolution {
        let Err(block) = self.sale_eligibility(id) else {
            return SaleResolution::None;
        };
        let equipped = |mode: Mode| {
            self.campaign
                .item(id)
                .map(|item| item.definition().slot)
                .filter(|slot| self.starter_item(mode, *slot).is_some_and(|s| s != id))
                .map_or(SaleResolution::None, |slot| SaleResolution::EquipStarter {
                    mode,
                    slot,
                })
        };
        match block {
            SaleBlock::Locked => SaleResolution::Unlock,
            SaleBlock::Favorite => SaleResolution::Unfavourite,
            SaleBlock::EquippedPersonal => equipped(Mode::Personal),
            SaleBlock::EquippedMara => equipped(Mode::Npc),
            SaleBlock::Preset { .. } => self
                .campaign
                .presets
                .iter()
                .position(|preset| preset.loadout.contains(&id))
                .map_or(SaleResolution::None, |index| SaleResolution::OpenPreset {
                    index,
                }),
            SaleBlock::RecruitEquipped { .. } => self
                .strategy
                .roster
                .iter()
                .find(|recruit| recruit.npc.loadout.contains(&id))
                .map_or(SaleResolution::None, |recruit| {
                    SaleResolution::OpenRecruit {
                        recruit: recruit.id,
                    }
                }),
            SaleBlock::Unknown
            | SaleBlock::InBattle
            | SaleBlock::Reserved
            | SaleBlock::Protected
            | SaleBlock::NpcOnly => SaleResolution::None,
        }
    }
    /// The command [`Game::sale_resolution`]'s shortcut dispatches for `id`:
    /// unlock it, drop its favourite mark, put the loadout's starter back on
    /// in its slot, or delete the preset that holds it. `None` where there
    /// is no shortcut and for a recruit's gear, which is changed on
    /// Operations. Applying it lifts exactly the guard that blocked the
    /// sale; a later guard may then show. Pure.
    pub fn sale_resolution_command(&self, id: u64) -> Option<Command> {
        match self.sale_resolution(id) {
            SaleResolution::Unlock => Some(Command::Lock { id, locked: false }),
            SaleResolution::Unfavourite => Some(Command::Favorite {
                id,
                favorite: false,
            }),
            SaleResolution::EquipStarter { mode, slot } => self
                .starter_item(mode, slot)
                .map(|id| Command::Equip { mode, id }),
            SaleResolution::OpenPreset { index } => Some(Command::DeletePreset { index }),
            SaleResolution::OpenRecruit { .. } | SaleResolution::None => None,
        }
    }
    /// The journey's own protected starter `mode`'s owner can wear in
    /// `slot`: the lowest id that is not a recruit's issue kit, not held by
    /// an expedition or a guard, and that the owner may use.
    fn starter_item(&self, mode: Mode, slot: Slot) -> Option<u64> {
        self.campaign
            .items
            .iter()
            .filter(|item| item.protected && item.definition().slot == slot)
            .filter(|item| !self.campaign.reserved_items.contains(&item.id))
            .filter(|item| {
                self.strategy
                    .roster
                    .iter()
                    .all(|recruit| !recruit.npc.loadout.contains(&item.id))
            })
            .filter(|item| {
                self.campaign
                    .weapon_access(mode, item.definition_index())
                    .is_ok()
            })
            .map(|item| item.id)
            .min()
    }
}
