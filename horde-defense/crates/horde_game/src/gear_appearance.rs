//! Read-only compatibility adapter. Old deployments can carry frozen gear
//! records without the newer visual snapshot; never inspect today's inventory.
use horde_core::{
    campaign::DefenderProfile,
    content::Slot,
    stats::GearRecord,
    survivors::{EquipmentVisual, EquipmentVisualItem},
};
use std::borrow::Cow;

pub fn from_frozen_gear<'a>(
    profile: &'a DefenderProfile,
    records: &[GearRecord],
) -> Cow<'a, EquipmentVisual> {
    let visual = &profile.equipment_visual;
    if *visual != EquipmentVisual::default() || records.is_empty() {
        return Cow::Borrowed(visual);
    }
    let slot = |slot| {
        let mut candidates = records.iter().filter(|record| record.slot == slot);
        let record = candidates.next()?;
        if candidates.next().is_some() || !(1..=64).contains(&record.level) {
            return None;
        }
        let definition = record.definition.definition()?;
        if definition.slot != slot {
            return None;
        }
        Some(EquipmentVisualItem {
            definition: record.definition.clone(),
            level: record.level,
            quality: record.quality,
            style: definition.visual_style.clone(),
        })
    };
    Cow::Owned(EquipmentVisual {
        ranged: slot(Slot::Ranged),
        outfit: slot(Slot::Outfit),
        accessory: slot(Slot::Accessory),
        melee: slot(Slot::Melee),
        weapon_class: Some(profile.weapon.class),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use horde_core::{content::Mode, sim::Game};

    #[test]
    fn legacy_rendering_recovers_only_deployed_gear_without_changing_the_game() {
        let mut game = Game::default();
        game.campaign.choose_start_country("DEU").unwrap();
        game.deploy_country_stage(0, Mode::Personal).unwrap();
        let attempt = game.attempt.as_ref().unwrap();
        let expected = attempt.profile.equipment_visual.clone();
        let mut legacy = attempt.profile.clone();
        legacy.equipment_visual = EquipmentVisual::default();
        let before = serde_json::to_value(&game).unwrap();
        assert_eq!(*from_frozen_gear(&legacy, &attempt.gear_snapshot), expected);
        assert_eq!(serde_json::to_value(&game).unwrap(), before);
        assert!(matches!(from_frozen_gear(&legacy, &[]), Cow::Borrowed(_)));
        assert!(matches!(
            from_frozen_gear(&attempt.profile, &[]),
            Cow::Borrowed(_)
        ));
    }

    #[test]
    fn legacy_adapter_never_replaces_a_new_snapshot_or_invents_a_mismatched_slot() {
        let campaign = horde_core::campaign::Campaign::default();
        let profile = campaign.profile(Mode::Personal);
        assert!(matches!(from_frozen_gear(&profile, &[]), Cow::Borrowed(_)));
        let mut legacy = profile.clone();
        legacy.equipment_visual = EquipmentVisual::default();
        let invalid = GearRecord {
            slot: Slot::Accessory,
            definition: horde_core::content::ItemDefId::at(0),
            name: "A name must never select the model".into(),
            level: 1,
            quality: horde_core::quality::Quality::Common,
            weapon_class: None,
        };
        assert!(from_frozen_gear(&legacy, &[invalid]).accessory.is_none());
    }
}
