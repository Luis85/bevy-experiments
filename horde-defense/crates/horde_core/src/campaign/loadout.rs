//! Defender profile derivation, item comparison and recommendations.

use super::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LoadoutMetrics {
    pub damage: i32,
    pub melee: i32,
    pub armor: i32,
    pub health: i32,
    pub stamina: i32,
    pub magazine: u32,
    pub reload: f32,
    pub shots_per_second: f32,
    pub weak_bonus: f32,
    pub guns: i32,
    pub dice: u8,
    pub adds: i32,
    pub dodge: i32,
    pub expected_basic_damage: f32,
    pub baseline_shots_to_kill: Option<u32>,
    pub melee_dice: u8,
    pub melee_adds: i32,
    pub melee_skill: i32,
    pub accuracy: i32,
    pub damage_kind: crate::rules::DamageKind,
    pub melee_kind: crate::rules::DamageKind,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ItemComparison {
    pub item_id: u64,
    pub slot: Slot,
    pub before: LoadoutMetrics,
    pub after: LoadoutMetrics,
    pub explanation: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecommendationAction {
    ChooseCountry,
    AllocateBody(BodyAttribute),
    Equip(u64),
    BuyEquipment(usize),
    LearnSkill(Skill),
    LearnNode(String),
    TrainNpc(NpcTraining),
    BuyTraining,
    BuyBlueprint(StructureKind),
    Deploy,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recommendation {
    pub title: String,
    pub detail: String,
    pub action: RecommendationAction,
    pub price: u64,
    pub affordable: bool,
}

pub(crate) fn mean_basic_damage(damage: crate::rules::Damage) -> f32 {
    let minimum = damage.minimum();
    if i32::from(damage.dice) + damage.adds >= minimum {
        return 3.5 * f32::from(damage.dice) + damage.adds as f32;
    }
    // Exact bounded convolution for authored negative modifiers: taking the
    // floor after averaging would misrepresent the minimum-damage procedure.
    let mut counts = vec![1u64];
    for _ in 0..damage.dice {
        let mut next = vec![0u64; counts.len() + 6];
        for (sum, count) in counts.iter().enumerate() {
            for face in 1..=6 {
                next[sum + face] += count;
            }
        }
        counts = next;
    }
    let total = counts.iter().sum::<u64>() as f64;
    (counts
        .iter()
        .enumerate()
        .map(|(sum, count)| *count as f64 * (sum as i32 + damage.adds).max(minimum) as f64)
        .sum::<f64>()
        / total) as f32
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DefenderProfile {
    #[serde(default)]
    pub equipment_visual: crate::survivors::EquipmentVisual,
    #[serde(default)]
    pub appearance: crate::survivors::Appearance,
    #[serde(default)]
    pub weapon: WeaponSpec,
    #[serde(default)]
    pub modifiers: SkillModifiers,
    pub personal: PersonalProfile,
    pub npc: NpcProfile,
    pub stats: Stats,
    pub ranged_skill: i32,
    pub melee_skill: i32,
    pub max_hp: i32,
    pub max_fp: i32,
    pub npc_melee_damage_dice: u8,
    pub npc_melee_damage_add: i32,
    pub npc_melee_kind: crate::rules::DamageKind,
    pub pierce: bool,
    pub sweep: bool,
    pub heal: bool,
    pub brace: bool,
    pub engineering: bool,
    pub strength: u32,
    pub endurance: u32,
    pub perception: u32,
}

impl Campaign {
    fn equipment_visual_for(
        &self,
        loadout: [u64; 4],
        class: WeaponClass,
    ) -> crate::survivors::EquipmentVisual {
        let item = |slot: Slot| {
            self.item(loadout[slot.index()]).map(|item| {
                let definition = item.definition();
                crate::survivors::EquipmentVisualItem {
                    definition: item.definition.clone(),
                    level: item.level,
                    quality: item.quality.unwrap_or(definition.base_quality),
                    style: definition.visual_style.clone(),
                }
            })
        };
        crate::survivors::EquipmentVisual {
            ranged: item(Slot::Ranged),
            outfit: item(Slot::Outfit),
            accessory: item(Slot::Accessory),
            melee: item(Slot::Melee),
            weapon_class: Some(class),
        }
    }
    pub fn profile(&self, mode: Mode) -> DefenderProfile {
        let loadout = match mode {
            Mode::Personal => self.player.loadout,
            Mode::Npc => self.npc.loadout,
        };
        self.profile_for_loadout(mode, loadout, &self.npc)
    }
    pub fn profile_for_loadout(
        &self,
        mode: Mode,
        loadout: [u64; 4],
        character: &Npc,
    ) -> DefenderProfile {
        let equipment = |slot: Slot| {
            let definition = self
                .item(loadout[slot.index()])
                .map_or(&content().items[slot.index()], Item::definition);
            let level = self
                .item(loadout[slot.index()])
                .map_or(1, |item| item.level);
            crate::quality::apply(
                crate::progression::scale_item(definition.clone(), level),
                self.item(loadout[slot.index()])
                    .and_then(|item| item.quality),
            )
        };
        let weapon = WeaponSpec::from_item(&equipment(Slot::Ranged));
        let modifiers = self.modifiers();
        let mut personal = equipment(Slot::Ranged).personal.clone();
        let mut npc = equipment(Slot::Ranged).npc.clone();
        let melee = equipment(Slot::Melee);
        let base_melee = crate::rules::Damage {
            dice: melee.npc.damage_dice,
            adds: melee.npc.damage_add,
            kind: melee.npc.damage_kind,
        };
        let npc_melee =
            crate::rules::strength_adjusted_swing(base_melee, character.st).unwrap_or(base_melee);
        personal.melee_damage = melee.personal.melee_damage;
        personal.health = 100;
        personal.stamina = 100;
        for slot in [Slot::Outfit, Slot::Accessory] {
            let i = equipment(slot);
            personal.armor += i.personal.armor;
            personal.health += i.personal.health;
            personal.stamina += i.personal.stamina;
            personal.weak_bonus += i.personal.weak_bonus;
            npc.armor += i.npc.armor;
            npc.accuracy += i.npc.accuracy;
            npc.skill_bonus += i.npc.skill_bonus;
        }
        let b = BodyAttribute::ALL.map(|attribute| {
            self.player.body[attribute].max(modifiers.body_development[attribute])
        });
        if mode == Mode::Personal {
            personal.health += b[0] as i32 * 5;
            personal.melee_damage += b[1] as i32 * 5;
            personal.reload = (personal.reload * (1.0 - 0.025 * b[2] as f32)).max(0.65);
            personal.stamina += b[3] as i32 * 5;
            personal.weak_bonus = (personal.weak_bonus + 0.03 * b[4] as f32).min(1.0);
            personal.reload = (personal.reload * modifiers.reload_multiplier).max(0.65);
            personal.cadence = (personal.cadence * modifiers.cadence_multiplier).max(0.08);
            personal.damage = ((personal.damage + modifiers.damage_add) as f32
                * modifiers.damage_multiplier)
                .round()
                .clamp(1.0, 100_000.0) as i32;
            personal.health += modifiers.health_add;
            personal.stamina += modifiers.stamina_add;
            personal.armor += modifiers.armor_add;
            personal.melee_damage += modifiers.melee_add;
        }
        let is_personal = mode == Mode::Personal;
        DefenderProfile {
            equipment_visual: self.equipment_visual_for(loadout, weapon.class),
            appearance: if mode == Mode::Npc {
                crate::survivors::Appearance::Female
            } else {
                self.selected_survivor
                    .map_or(crate::survivors::Appearance::Male, |id| id.appearance())
            },
            weapon,
            modifiers: modifiers.clone(),
            personal,
            npc,
            stats: character.stats(),
            ranged_skill: character.guns,
            melee_skill: character.melee,
            max_hp: character.max_hp()
                + if mode == Mode::Npc {
                    ([Slot::Outfit, Slot::Accessory]
                        .into_iter()
                        .map(|slot| {
                            self.item(loadout[slot.index()])
                                .map_or(0, |item| item.level.saturating_sub(1))
                        })
                        .sum::<u32>() as f32
                        * crate::progression::catalog().gear.npc_health_per_level)
                        .floor() as i32
                } else {
                    0
                },
            max_fp: character.max_fp(),
            npc_melee_damage_dice: npc_melee.dice,
            npc_melee_damage_add: npc_melee.adds,
            npc_melee_kind: melee.npc.damage_kind,
            pierce: is_personal && modifiers.unlocked(unlocks::PIERCING),
            sweep: is_personal && modifiers.unlocked(unlocks::SWEEP),
            heal: is_personal && modifiers.unlocked(unlocks::HEAL),
            brace: is_personal && modifiers.unlocked(unlocks::BRACE),
            engineering: modifiers.engineering,
            strength: if is_personal { b[1] } else { 0 },
            endurance: if is_personal { b[3] } else { 0 },
            perception: if is_personal { b[4] } else { 0 },
        }
    }
    pub fn compare_item(&self, mode: Mode, id: u64) -> Result<ItemComparison, String> {
        let candidate = self.item(id).ok_or("Item is no longer owned")?;
        let slot = candidate
            .definition
            .definition()
            .ok_or("Unknown item profile")?
            .slot;
        let before = if mode == Mode::Personal {
            self.player.loadout
        } else {
            self.npc.loadout
        };
        let mut after = before;
        after[slot.index()] = id;
        self.validate_loadout(after)?;
        let metrics = |loadout| {
            let p = self.profile_for_loadout(mode, loadout, &self.npc);
            if mode == Mode::Personal {
                let health = content().enemy(crate::content::EnemyKind::Shambler).health as u32;
                let damage = ((p.personal.damage
                    - content().enemy(crate::content::EnemyKind::Shambler).armor)
                    .max(1) as u32
                    * p.weapon.pellets)
                    .min(p.weapon.hit_cap(health as i32) as u32)
                    .max(1);
                LoadoutMetrics {
                    damage: p.personal.damage,
                    melee: p.personal.melee_damage,
                    armor: p.personal.armor,
                    health: p.personal.health,
                    stamina: p.personal.stamina,
                    magazine: p.personal.magazine,
                    reload: p.personal.reload,
                    shots_per_second: 1.0 / p.personal.cadence.max(0.01),
                    weak_bonus: p.personal.weak_bonus,
                    guns: 0,
                    dice: 0,
                    adds: 0,
                    dodge: 0,
                    expected_basic_damage: p.personal.damage as f32,
                    baseline_shots_to_kill: Some(health.div_ceil(damage)),
                    melee_dice: 0,
                    melee_adds: 0,
                    melee_skill: 0,
                    accuracy: 0,
                    damage_kind: p.npc.damage_kind,
                    melee_kind: p.npc_melee_kind,
                }
            } else {
                LoadoutMetrics {
                    damage: 0,
                    melee: 0,
                    armor: p.npc.armor,
                    health: p.max_hp,
                    stamina: p.max_fp,
                    magazine: p.npc.magazine,
                    reload: p.npc.reload_turns as f32,
                    shots_per_second: 1.0,
                    weak_bonus: 0.0,
                    guns: p.ranged_skill + p.npc.skill_bonus,
                    dice: p.npc.damage_dice,
                    adds: p.npc.damage_add,
                    dodge: p.stats.dodge(),
                    expected_basic_damage: mean_basic_damage(crate::rules::Damage {
                        dice: p.npc.damage_dice,
                        adds: p.npc.damage_add,
                        kind: p.npc.damage_kind,
                    }),
                    baseline_shots_to_kill: None,
                    melee_dice: p.npc_melee_damage_dice,
                    melee_adds: p.npc_melee_damage_add,
                    melee_skill: p.melee_skill,
                    accuracy: p.npc.accuracy,
                    damage_kind: p.npc.damage_kind,
                    melee_kind: p.npc_melee_kind,
                }
            }
        };
        Ok(ItemComparison{item_id:id,slot,before:metrics(before),after:metrics(after),explanation:if mode==Mode::Personal{"Personal body-hit profile; Shambler breakpoint excludes weak points and country modifiers."}else{"NPC preparation profile before country modifiers. Weapon mean is basic damage before attack rolls, active defense, armor and wounding; not a chance to injure."}.into()})
    }
    pub fn recommendation(&self, mode: Mode) -> Recommendation {
        let make = |title: String, detail: String, action, price| Recommendation {
            title,
            detail,
            action,
            price,
            affordable: self.coins >= price,
        };
        if self.world.start.is_none() {
            return make("Choose your starting country".into(),"Select any country, then prepare its first defense. Earned coins and loot survive defeat.".into(),RecommendationAction::ChooseCountry,0);
        }
        if self
            .world
            .active
            .as_ref()
            .is_some_and(|id| self.world.cleared.contains(id))
        {
            let next = crate::world::world()
                .countries
                .iter()
                .filter(|c| self.world.can_enter(&c.id) && !self.world.cleared.contains(&c.id))
                .min_by(|a, b| a.difficulty.total_cmp(&b.difficulty).then(a.id.cmp(&b.id)));
            if let Some(country) = next {
                return make(
                    "Open a new neighboring front".into(),
                    format!(
                        "{} is accessible after liberation: {} local defenses. Select an unlocked country on the map; already cleared defenses remain replayable.",
                        country.name,
                        country.level_count()
                    ),
                    RecommendationAction::ChooseCountry,
                    0,
                );
            }
            return make("The configured world is liberated".into(),"Replay cleared defenses, prepare survivors and watch for strategic plague alerts. Earned travel routes remain open.".into(),RecommendationAction::Deploy,0);
        }
        if mode == Mode::Personal
            && self.legacy_features
            && self.player.points > 0
            && let Some(node) = skills().nodes.iter().find(|node| {
                node.effects.iter().any(|effect| {
                    matches!(
                        effect,
                        crate::skill_catalog::SkillEffect::BodyDevelopment { .. }
                    )
                }) && self.can_purchase_node(&node.id).is_ok()
                    && matches!(
                        self.node_payment(&node.id),
                        Ok(NodePayment::LegacyBodyPoints(_))
                    )
            })
        {
            return make("Use a retained legacy body point".into(),"Legacy points remain spendable through body nodes in the same skill graph. No coins are charged for this adapter.".into(),RecommendationAction::LearnNode(node.id.clone()),0);
        }
        let current = self.profile(mode);
        let score = |m: &LoadoutMetrics, slot: Slot| match slot {
            Slot::Ranged => {
                if mode == Mode::Personal {
                    m.damage as f32 * m.shots_per_second + 2.0 * m.magazine as f32 - 5.0 * m.reload
                } else {
                    m.expected_basic_damage * 8.0 + m.guns as f32 * 3.0 + m.magazine as f32
                }
            }
            Slot::Melee => {
                if mode == Mode::Personal {
                    m.melee as f32
                } else {
                    (3.5 * m.melee_dice as f32 + m.melee_adds as f32) * 4.0 + m.melee_skill as f32
                }
            }
            Slot::Outfit => m.armor as f32 * 10.0 + m.health as f32,
            Slot::Accessory => {
                m.health as f32
                    + m.stamina as f32
                    + m.armor as f32 * 10.0
                    + m.weak_bonus * 40.0
                    + m.accuracy as f32 * 4.0
            }
        };
        let owned = self
            .items
            .iter()
            .filter(|i| {
                !self.item_reserved(i.id) && self.can_use_weapon(mode, i.definition_index())
            })
            .filter_map(|item| self.compare_item(mode, item.id).ok())
            .filter(|c| score(&c.after, c.slot) > score(&c.before, c.slot) + 1.0)
            .max_by(|a, b| {
                (score(&a.after, a.slot) - score(&a.before, a.slot))
                    .total_cmp(&(score(&b.after, b.slot) - score(&b.before, b.slot)))
            });
        if let Some(item) = owned {
            return make(
                "Equip an improvement already in your armory".into(),
                format!(
                    "{} improves your selected {:?} loadout. Compare the complete profile before equipping.",
                    self.item(item.item_id)
                        .expect("owned comparison")
                        .definition()
                        .name,
                    mode
                ),
                RecommendationAction::Equip(item.item_id),
                0,
            );
        }
        if mode == Mode::Npc && self.npc.unspent >= NpcTraining::Guns.cost() {
            return make(
                "Improve Mara's ranged skill".into(),
                format!(
                    "Spend 4 unspent NPC points: Guns {} to {}. Awarded points remain inside the campaign allowance.",
                    self.npc.guns,
                    self.npc.guns + 1
                ),
                RecommendationAction::TrainNpc(NpcTraining::Guns),
                0,
            );
        }
        for kind in StructureKind::ALL {
            if self.blueprint_available(kind) && !self.blueprints[kind] {
                let def = content().structure(kind);
                return make(
                    format!("Unlock {}", def.name),
                    format!(
                        "Blueprint ownership is permanent. Placement uses renewed attempt supply, not your wallet. {} coins required.",
                        def.blueprint_price
                    ),
                    RecommendationAction::LearnNode(
                        match kind {
                            StructureKind::Trap => "trap_blueprint",
                            StructureKind::Wall => "wall_blueprint",
                            StructureKind::Turret => "turret_blueprint",
                        }
                        .into(),
                    ),
                    def.blueprint_price,
                );
            }
        }
        if mode == Mode::Personal && !self.has_skill(Skill::QuickReload) {
            let node = skills()
                .node("quick_reload")
                .expect("validated compatibility node");
            return make(
                format!("Learn {}", node.name),
                format!(
                    "{} coins. {} Rewards survive short attempts.",
                    node.price, node.description
                ),
                RecommendationAction::LearnNode(node.id.clone()),
                node.price,
            );
        }
        if mode == Mode::Personal {
            for item in &self.items {
                if self.can_use_weapon(mode, item.definition_index())
                    || item.definition().slot != Slot::Ranged
                    || (item.definition().npc_only && !self.legacy_weapon_training)
                {
                    continue;
                }
                let class = WeaponSpec::from_item(item.definition()).class;
                if let Some(node) = skills().nodes.iter().find(|n| !self.has_node(&n.id) && n.prerequisites.iter().all(|p| self.has_node(p)) && n.effects.iter().any(|e| matches!(e, crate::skill_catalog::SkillEffect::WeaponTraining(c) if *c == class))) {
                    return make(format!("Train for your owned {}", item.definition().name), format!("{} Unlocking proficiency permits equipping the weapon already in your armory.", node.description), RecommendationAction::LearnNode(node.id.clone()), node.price);
                }
            }
        }
        let slot = if (mode == Mode::Personal && current.personal.armor < 3)
            || (mode == Mode::Npc && current.npc.armor < 3)
        {
            Slot::Outfit
        } else {
            Slot::Ranged
        };
        let current_item = self.equipped(mode, slot);
        let current_level = self
            .item(if mode == Mode::Personal {
                self.player.loadout[slot.index()]
            } else {
                self.npc.loadout[slot.index()]
            })
            .map_or(1, |item| item.level);
        let desired = content()
            .items
            .iter()
            .enumerate()
            .filter(|(index, i)| {
                self.can_use_weapon(mode, *index)
                    && i.slot == slot
                    && (i.tier > current_item.tier
                        || (i.tier == current_item.tier && self.shop_item_level() > current_level))
                    && i.tier <= self.unlocked
                    && i.buy_value > 0
            })
            .min_by_key(|(_, i)| i.buy_value);
        if let Some((index, item)) = desired {
            let price = self.buy_price(index).expect("known shop definition");
            return make(
                format!("Save for {}", item.name),
                format!(
                    "Guaranteed quartermaster gear costs {} coins; {} more needed. No rare drop is required.",
                    price,
                    price.saturating_sub(self.coins)
                ),
                RecommendationAction::BuyEquipment(index),
                price,
            );
        }
        if mode == Mode::Npc && self.npc.awarded < self.allowance() {
            return make("Fund Mara's next training point".into(),"Training is immediate in camp. Four Guns points buy another skill level; earned objectives are claimed first.".into(),RecommendationAction::BuyTraining,25);
        }
        let detail=self.world.active_country().map_or_else(||"Choose an unlocked country on the map.".to_owned(),|country|format!("{}: defense {} of {}. Earlier cleared defenses remain replayable; rewards are banked immediately.",country.name,self.world.next_stage()+1,country.level_count()));
        make(
            "Prepare the next defense".into(),
            detail,
            RecommendationAction::Deploy,
            0,
        )
    }
}
