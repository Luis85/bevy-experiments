//! Armory presentation. Game changes are queued typed intents.
use super::{Tab, UiCtx, chrome, mode_selector, muted};
use crate::art::{AMBER, GREEN, INK, RED, TEXT};
use bevy_egui::egui::{self, Align, Color32, Layout, Rect, RichText, Sense, Stroke, vec2};
use horde_core::{
    command::Command,
    content::{Mode, Slot, content},
};
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ArmorySort {
    Newest,
    Name,
    Tier,
    Level,
    Quality,
}
impl ArmorySort {
    fn name(self) -> &'static str {
        match self {
            Self::Newest => "Newest first",
            Self::Name => "Name",
            Self::Tier => "Highest tier",
            Self::Level => "Highest item level",
            Self::Quality => "Highest quality",
        }
    }
}

pub(super) fn quality_color(quality: horde_core::quality::Quality) -> Color32 {
    let [r, g, b, a] = quality.color();
    Color32::from_rgba_unmultiplied(
        (r * 255.0).round() as u8,
        (g * 255.0).round() as u8,
        (b * 255.0).round() as u8,
        (a * 255.0).round() as u8,
    )
}
pub(super) fn loot_receipt(ui: &mut egui::Ui, receipt: &horde_core::quality::LootEvent) {
    let text = format!(
        "{} · {} · Lv {}",
        receipt.quality.name(),
        receipt.name,
        receipt.level
    );
    ui.add(egui::Label::new(RichText::new(&text).color(quality_color(receipt.quality))).truncate())
        .on_hover_text(format!(
            "{text} · owned #{} · already banked",
            receipt.item_id
        ));
}
pub(super) fn paint_item_icon(
    p: &egui::Painter,
    textures: crate::visual_assets::ArtTextures,
    definition: usize,
    rect: Rect,
    tint: Color32,
) {
    let item = &content().items[definition];
    if item.slot == Slot::Ranged
        && crate::visual_assets::firearm_icon(
            p,
            textures,
            horde_core::weapons::WeaponSpec::from_item(item).class,
            rect.center(),
            rect.size(),
            tint,
        )
    {
        return;
    }
    let prop = match item.slot {
        Slot::Ranged => crate::visual_assets::Prop::Rifle,
        Slot::Melee => crate::visual_assets::Prop::Melee,
        Slot::Outfit => crate::visual_assets::Prop::Vest,
        Slot::Accessory => crate::visual_assets::Prop::Pack,
    };
    crate::visual_assets::prop(p, textures, prop, rect.center(), rect.size(), tint);
}
fn slot_art(
    ui: &mut egui::Ui,
    f: &UiCtx,
    definition: usize,
    quality: horde_core::quality::Quality,
) {
    let (rect, _) = ui.allocate_exact_size(vec2(36.0, 36.0), Sense::hover());
    paint_item_icon(
        ui.painter(),
        f.textures,
        definition,
        rect.shrink(3.0),
        Color32::WHITE,
    );
    ui.painter().rect_stroke(
        rect,
        0.0,
        Stroke::new(1.5, quality_color(quality)),
        egui::StrokeKind::Inside,
    );
}
fn inventory_comparison(ui: &mut egui::Ui, f: &UiCtx, id: u64) {
    let Some(item) = f.game.campaign.item(id) else {
        return;
    };
    let definition = &content().items[item.definition];
    let quality = f.game.campaign.item_quality(id).expect("owned equipment");
    let Ok(comparison) = f.game.campaign.compare_item(f.campaign.mode, id) else {
        return;
    };
    let before = &comparison.before;
    let after = &comparison.after;
    ui.horizontal(|ui| {
        slot_art(ui, f, item.definition, quality);
        ui.vertical(|ui| {
            ui.label(
                RichText::new(format!("{} #{} · Lv {}", definition.name, id, item.level))
                    .strong()
                    .color(quality_color(quality)),
            );
            muted(
                ui,
                format!(
                    "{} quality · {:?} · unlock tier {}",
                    quality.name(),
                    definition.slot,
                    definition.tier
                ),
            );
        });
    });
    egui::Grid::new(("item_comparison", id))
        .num_columns(3)
        .spacing([18.0, 6.0])
        .show(ui, |ui| {
            ui.label("Loadout effect");
            ui.strong("Equipped");
            ui.strong("With this item");
            ui.end_row();
            let mut row = |label: &str, old: String, new: String, better: bool, worse: bool| {
                ui.label(label);
                ui.label(old);
                ui.colored_label(
                    if better {
                        GREEN
                    } else if worse {
                        RED
                    } else {
                        TEXT
                    },
                    new,
                );
                ui.end_row();
            };
            if f.campaign.mode == Mode::Personal {
                row(
                    "Bullet damage",
                    before.damage.to_string(),
                    after.damage.to_string(),
                    after.damage > before.damage,
                    after.damage < before.damage,
                );
                row(
                    "Melee damage",
                    before.melee.to_string(),
                    after.melee.to_string(),
                    after.melee > before.melee,
                    after.melee < before.melee,
                );
                row(
                    "Shots / second",
                    format!("{:.2}", before.shots_per_second),
                    format!("{:.2}", after.shots_per_second),
                    after.shots_per_second > before.shots_per_second,
                    after.shots_per_second < before.shots_per_second,
                );
                if let (Some(old), Some(new)) =
                    (before.baseline_shots_to_kill, after.baseline_shots_to_kill)
                {
                    row(
                        "Shambler body hits",
                        old.to_string(),
                        new.to_string(),
                        new < old,
                        new > old,
                    );
                }
            } else {
                row(
                    "Ranged profile",
                    format!("{}d{:+} {:?}", before.dice, before.adds, before.damage_kind),
                    format!("{}d{:+} {:?}", after.dice, after.adds, after.damage_kind),
                    after.expected_basic_damage > before.expected_basic_damage,
                    after.expected_basic_damage < before.expected_basic_damage,
                );
                row(
                    "Melee profile",
                    format!(
                        "{}d{:+} {:?}",
                        before.melee_dice, before.melee_adds, before.melee_kind
                    ),
                    format!(
                        "{}d{:+} {:?}",
                        after.melee_dice, after.melee_adds, after.melee_kind
                    ),
                    false,
                    false,
                );
                row(
                    "Guns skill",
                    before.guns.to_string(),
                    after.guns.to_string(),
                    after.guns > before.guns,
                    after.guns < before.guns,
                );
                row(
                    "Accuracy after Aim",
                    before.accuracy.to_string(),
                    after.accuracy.to_string(),
                    after.accuracy > before.accuracy,
                    after.accuracy < before.accuracy,
                );
            }
            if f.campaign.mode == Mode::Personal {
                row(
                    "Weak-point bonus",
                    format!("{:.0}%", before.weak_bonus * 100.0),
                    format!("{:.0}%", after.weak_bonus * 100.0),
                    after.weak_bonus > before.weak_bonus,
                    after.weak_bonus < before.weak_bonus,
                );
            } else {
                row(
                    "Mean basic ranged damage",
                    format!("{:.2}", before.expected_basic_damage),
                    format!("{:.2}", after.expected_basic_damage),
                    after.expected_basic_damage > before.expected_basic_damage,
                    after.expected_basic_damage < before.expected_basic_damage,
                );
                row(
                    "Melee skill",
                    before.melee_skill.to_string(),
                    after.melee_skill.to_string(),
                    after.melee_skill > before.melee_skill,
                    after.melee_skill < before.melee_skill,
                );
                row(
                    "Dodge",
                    before.dodge.to_string(),
                    after.dodge.to_string(),
                    after.dodge > before.dodge,
                    after.dodge < before.dodge,
                );
            }
            row(
                if f.campaign.mode == Mode::Personal {
                    "Stamina"
                } else {
                    "Fatigue points"
                },
                before.stamina.to_string(),
                after.stamina.to_string(),
                after.stamina > before.stamina,
                after.stamina < before.stamina,
            );
            row(
                "Protection",
                before.armor.to_string(),
                after.armor.to_string(),
                after.armor > before.armor,
                after.armor < before.armor,
            );
            row(
                "Maximum health",
                before.health.to_string(),
                after.health.to_string(),
                after.health > before.health,
                after.health < before.health,
            );
            row(
                "Magazine",
                before.magazine.to_string(),
                after.magazine.to_string(),
                after.magazine > before.magazine,
                after.magazine < before.magazine,
            );
            row(
                "Reload seconds",
                format!("{:.2}", before.reload),
                format!("{:.2}", after.reload),
                after.reload < before.reload,
                after.reload > before.reload,
            );
        });
    muted(ui, &comparison.explanation);
    if definition.npc_only {
        muted(ui, "NPC issued gear");
    }
    if definition.slot == Slot::Ranged {
        let weapon = horde_core::weapons::WeaponSpec::from_item(definition);
        muted(
            ui,
            format!(
                "{} · {} projectile(s) per shot · {:.0}° spread",
                weapon.class.name(),
                weapon.pellets,
                weapon.spread_radians.to_degrees()
            ),
        );
        if weapon.max_full_health_fraction_per_hit < 1.0 {
            muted(
                ui,
                "Pistol hits cannot defeat a fresh enemy in one trigger pull, including weak points and training bonuses.",
            );
        }
    }
    if f.campaign.mode == Mode::Npc {
        muted(
            ui,
            "Weapon dice describe basic damage before protection. Attack success, defense and injury remain separate stages.",
        );
    }
}
pub(super) fn armory(ui: &mut egui::Ui, f: &mut UiCtx) {
    chrome::page_title(
        ui,
        "Equipment",
        "Choose equipment for the next defense. Deployed and guarding survivors reserve their unique items.",
    );
    mode_selector(ui, f);
    let rank = f.game.campaign.progression_rank();
    let level = f.game.campaign.shop_item_level();
    let tuning = horde_core::progression::catalog();
    let next = level.saturating_mul(tuning.item_level_rank_step);
    let reachable_max = tuning.snapshot(tuning.threat_guard_rank).item_level;
    muted(
        ui,
        if level < reachable_max && next <= tuning.threat_guard_rank {
            format!(
                "New equipment: Lv {level} · threat rank {rank} · next gear level at rank {next}"
            )
        } else {
            format!("New equipment: Lv {level} · maximum authored item level reached")
        },
    );
    let recommended_purchase = match f.campaign.improvement_focus {
        Some(horde_core::campaign::RecommendationAction::BuyEquipment(index)) => Some(index),
        _ => None,
    };
    if let Some(horde_core::campaign::RecommendationAction::Equip(id)) =
        f.campaign.improvement_focus
        && f.game.campaign.item(id).is_some()
    {
        f.armory.selected_item = Some(id);
    }
    if let Some(index) = recommended_purchase
        && let Some(item) = content().items.get(index)
    {
        let price = f
            .game
            .campaign
            .buy_price(index)
            .expect("catalogue definition exists");
        ui.push_id("recommended_equipment", |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.strong(format!("Recommended: {}", item.name));
                if ui
                    .add_enabled(
                        item.tier <= f.game.campaign.unlocked && f.game.campaign.coins >= price,
                        egui::Button::new(format!("Buy · {} coins", price)),
                    )
                    .clicked()
                {
                    f.dispatch(
                        Command::BuyItem(index),
                        "Equipment added. Select it below to compare before equipping.",
                    );
                }
            });
            muted(
                ui,
                "Purchase this item for your next defense, then choose whether to equip it.",
            );
        });
    }
    ui.horizontal_wrapped(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut f.armory.search)
                .hint_text("Search equipment")
                .desired_width(180.0),
        );
        egui::ComboBox::from_id_salt("armory_slot")
            .selected_text(
                f.armory
                    .slot
                    .map(|s| format!("{s:?}"))
                    .unwrap_or_else(|| "All slots".into()),
            )
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut f.armory.slot, None, "All slots");
                for slot in Slot::ALL {
                    ui.selectable_value(&mut f.armory.slot, Some(slot), format!("{slot:?}"));
                }
            });
        egui::ComboBox::from_id_salt("armory_quality")
            .selected_text(
                f.armory
                    .quality
                    .map_or("All qualities", horde_core::quality::Quality::name),
            )
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut f.armory.quality, None, "All qualities");
                for quality in horde_core::quality::Quality::ALL {
                    ui.selectable_value(
                        &mut f.armory.quality,
                        Some(quality),
                        RichText::new(quality.name()).color(quality_color(quality)),
                    );
                }
            });
        egui::ComboBox::from_id_salt("armory_sort")
            .selected_text(f.armory.sort.name())
            .show_ui(ui, |ui| {
                for sort in [
                    ArmorySort::Newest,
                    ArmorySort::Name,
                    ArmorySort::Tier,
                    ArmorySort::Level,
                    ArmorySort::Quality,
                ] {
                    ui.selectable_value(&mut f.armory.sort, sort, sort.name());
                }
            });
    });
    ui.horizontal_wrapped(|ui| {
        for quality in horde_core::quality::Quality::ALL {
            ui.label(
                RichText::new(quality.name())
                    .size(12.0)
                    .color(quality_color(quality)),
            );
        }
    });
    let query = f.armory.search.trim().to_lowercase();
    // Definition matching is done once per definition, rather than allocating
    // lowercase names once per owned instance in an unlimited stash.
    let matches: Vec<_> = content()
        .items
        .iter()
        .map(|item| {
            query.is_empty()
                || item.name.to_lowercase().contains(&query)
                || item.rarity.to_lowercase().contains(&query)
        })
        .collect();
    let mut indices: Vec<_> = f
        .game
        .campaign
        .items
        .iter()
        .enumerate()
        .filter(|(_, item)| {
            matches[item.definition]
                && f.armory
                    .slot
                    .is_none_or(|slot| content().items[item.definition].slot == slot)
                && f.armory.quality.is_none_or(|quality| {
                    f.game.campaign.item_quality(item.id).ok() == Some(quality)
                })
        })
        .map(|(index, _)| index)
        .collect();
    indices.sort_by(|&a, &b| {
        let a = &f.game.campaign.items[a];
        let b = &f.game.campaign.items[b];
        match f.armory.sort {
            ArmorySort::Newest => b.id.cmp(&a.id),
            ArmorySort::Name => content().items[a.definition]
                .name
                .cmp(&content().items[b.definition].name)
                .then(a.id.cmp(&b.id)),
            ArmorySort::Level => b.level.cmp(&a.level).then(b.id.cmp(&a.id)),
            ArmorySort::Quality => f
                .game
                .campaign
                .item_quality(b.id)
                .expect("owned item")
                .cmp(&f.game.campaign.item_quality(a.id).expect("owned item"))
                .then(b.level.cmp(&a.level))
                .then(b.id.cmp(&a.id)),
            ArmorySort::Tier => content().items[b.definition]
                .tier
                .cmp(&content().items[a.definition].tier)
                .then(a.id.cmp(&b.id)),
        }
    });
    muted(
        ui,
        format!(
            "Showing {} of {} items · storage has no item limit",
            indices.len(),
            f.game.campaign.items.len()
        ),
    );

    // Selection is view state only. Never change equipment implicitly.
    if f.armory
        .selected_item
        .is_none_or(|id| f.game.campaign.item(id).is_none())
    {
        f.armory.selected_item = indices
            .first()
            .map(|index| f.game.campaign.items[*index].id);
    }
    if indices.is_empty() {
        muted(
            ui,
            "No matching equipment. Clear the search or choose another slot.",
        );
    }
    if ui.available_width() >= 920.0 {
        let available = ui.available_width();
        let list_width = available * 0.52;
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                vec2(list_width, 280.0),
                Layout::top_down(Align::Min),
                |ui| inventory_list(ui, f, &indices),
            );
            ui.separator();
            ui.allocate_ui_with_layout(
                vec2((available - list_width - 24.0).max(200.0), 280.0),
                Layout::top_down(Align::Min),
                |ui| {
                    selected_item(ui, f);
                },
            );
        });
    } else {
        inventory_list(ui, f, &indices);
        if let Some(id) = f.armory.selected_item {
            let name = f
                .game
                .campaign
                .item(id)
                .map(|item| content().items[item.definition].name.as_str())
                .unwrap_or("Item");
            let quality = f.game.campaign.item_quality(id).expect("owned item");
            ui.label(
                RichText::new(format!("Selected: {name} #{id} · {}", quality.name()))
                    .strong()
                    .color(quality_color(quality)),
            );
            selected_actions(ui, f);
            egui::CollapsingHeader::new("Compare complete loadout")
                .id_salt("compact_item_comparison")
                .show(ui, |ui| inventory_comparison(ui, f, id));
        }
    }
    chrome::section(ui, "Manage equipment");
    sale_confirmation(ui, f);
    egui::CollapsingHeader::new("Loadout presets").show(ui, |ui| {
        muted(ui, "Save references to your current equipment. Referenced items are retained by bulk sales.");
        ui.horizontal_wrapped(|ui| {
            ui.add(egui::TextEdit::singleline(&mut f.armory.preset_name).hint_text("Preset name").desired_width(180.0));
            if ui.button("Save loadout preset").clicked() { f.dispatch(Command::SavePreset { name: f.armory.preset_name.clone(), mode: f.campaign.mode }, "Loadout saved by reference."); }
            for (index, preset) in f.game.campaign.presets.clone().iter().enumerate() {
                if ui.button(format!("Use {}", preset.name)).clicked() { f.dispatch(Command::ApplyPreset { index, mode: f.campaign.mode }, "Loadout equipped."); }
            }
        });
    });
    egui::CollapsingHeader::new("Quartermaster · guaranteed equipment").open(recommended_purchase.map(|_| true)).show(ui, |ui| {
        muted(ui, "Progression never requires a rare drop. Purchases go to the armory; equip them separately.");
    let unlocked = f.game.campaign.unlocked;
    for (index, item) in content()
        .items
        .iter()
        .enumerate()
        .filter(|(_, item)| item.tier > 0 && item.tier <= unlocked)
    {
        let price = f.game.campaign.buy_price(index).expect("catalogue definition exists");
        let offered_level = f.game.campaign.shop_item_level();
        let offered = horde_core::progression::scale_item(item.clone(), offered_level);
        ui.horizontal(|ui| {
            slot_art(ui, f, index, item.base_quality);
            ui.label(RichText::new(format!("{} · {} · {:?} · Lv {}", item.name, item.base_quality.name(), item.slot, offered_level)).color(quality_color(item.base_quality))).on_hover_text(format!("Personal: {} damage · {} armor · {} health. NPC: {}d{:+} · {} armor. Stats include this offered item level.", offered.personal.damage, offered.personal.armor, offered.personal.health, offered.npc.damage_dice, offered.npc.damage_add, offered.npc.armor));
            if item.slot == Slot::Ranged {
                let weapon = horde_core::weapons::WeaponSpec::from_item(item);
                ui.label(format!("{} · {}", weapon.class.name(), if item.npc_only { "NPC issued gear" } else if f.game.campaign.can_use_weapon(f.campaign.mode, index) { "trained" } else { "training required" }));
            }
            if ui
                .add_enabled(
                    f.game.campaign.coins >= price,
                    egui::Button::new(format!("Buy · {} coins", price)),
                )
                .clicked()
            {
                f.dispatch(
                    Command::BuyItem(index),
                    "Equipment added. Select it above to compare before equipping.",
                );
            }
        });
    }
    });
}

fn inventory_list(ui: &mut egui::Ui, f: &mut UiCtx, indices: &[usize]) {
    let viewport = (ui.clip_rect().height() * 0.46).clamp(150.0, 330.0);
    egui::ScrollArea::vertical()
        .id_salt("armory")
        .max_height(viewport)
        .show_rows(ui, 72.0, indices.len(), |ui, range| {
            for index in range {
                let item = f.game.campaign.items[indices[index]].clone();
                let definition = &content().items[item.definition];
                let quality = f
                    .game
                    .campaign
                    .item_quality(item.id)
                    .expect("owned equipment");
                let loadout = if f.campaign.mode == Mode::Personal {
                    f.game.campaign.player.loadout
                } else {
                    f.game.campaign.npc.loadout
                };
                let equipped = loadout.contains(&item.id);
                let reserved = f.game.campaign.item_reserved(item.id);
                ui.push_id(item.id, |ui| {
                    ui.horizontal_top(|ui| {
                        slot_art(ui, f, item.definition, quality);
                        let name_width = (ui.available_width() - 100.0).max(150.0);
                        ui.allocate_ui_with_layout(
                            vec2(name_width, 62.0),
                            Layout::top_down(Align::Min),
                            |ui| {
                                // Rarity text retains contrast when selected; the
                                // ordinary amber selection fill muddies blue/purple.
                                ui.visuals_mut().selection.bg_fill = INK;
                                if ui
                                    .selectable_label(
                                        f.armory.selected_item == Some(item.id),
                                        RichText::new(format!(
                                            "{} #{} · Lv {}",
                                            definition.name, item.id, item.level
                                        ))
                                        .color(quality_color(quality)),
                                    )
                                    .clicked()
                                {
                                    f.armory.selected_item = Some(item.id);
                                }
                                muted(
                                    ui,
                                    format!(
                                        "{} · {:?} · {}{}{}{}",
                                        quality.name(),
                                        definition.slot,
                                        if reserved {
                                            "Reserved"
                                        } else if equipped {
                                            "Equipped"
                                        } else {
                                            "Owned"
                                        },
                                        if item.locked { " · locked" } else { "" },
                                        if item.favorite { " · favorite" } else { "" },
                                        if definition.npc_only {
                                            " · NPC issued gear"
                                        } else {
                                            ""
                                        }
                                    ),
                                );
                            },
                        );
                        if equipped {
                            ui.colored_label(GREEN, "Equipped");
                        } else if ui
                            .add_enabled(
                                !reserved
                                    && f.game
                                        .campaign
                                        .can_use_weapon(f.campaign.mode, item.definition),
                                egui::Button::new("Equip"),
                            )
                            .on_disabled_hover_text(if reserved {
                                "Reserved by a deployed survivor or guard."
                            } else {
                                f.game
                                    .campaign
                                    .weapon_access(f.campaign.mode, item.definition)
                                    .err()
                                    .unwrap_or_default()
                            })
                            .clicked()
                        {
                            f.dispatch(
                                Command::Equip {
                                    mode: f.campaign.mode,
                                    id: item.id,
                                },
                                "Equipment changed.",
                            );
                        }
                    });
                });
            }
        });
}

fn selected_item(ui: &mut egui::Ui, f: &mut UiCtx) {
    selected_actions(ui, f);
    if let Some(id) = f.armory.selected_item {
        inventory_comparison(ui, f, id);
    }
}

fn selected_actions(ui: &mut egui::Ui, f: &mut UiCtx) {
    let Some(item) = f
        .armory
        .selected_item
        .and_then(|id| f.game.campaign.item(id))
        .cloned()
    else {
        return;
    };
    let loadout = if f.campaign.mode == Mode::Personal {
        f.game.campaign.player.loadout
    } else {
        f.game.campaign.npc.loadout
    };
    let reserved = f.game.campaign.item_reserved(item.id);
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(
                !reserved
                    && !loadout.contains(&item.id)
                    && f.game
                        .campaign
                        .can_use_weapon(f.campaign.mode, item.definition),
                egui::Button::new("Equip selected"),
            )
            .on_disabled_hover_text(if reserved {
                "Reserved by a deployed survivor or guard."
            } else {
                f.game
                    .campaign
                    .weapon_access(f.campaign.mode, item.definition)
                    .err()
                    .unwrap_or_default()
            })
            .clicked()
        {
            f.dispatch(
                Command::Equip {
                    mode: f.campaign.mode,
                    id: item.id,
                },
                "Equipment changed.",
            );
        }
        let mut locked = item.locked;
        if ui
            .add_enabled(!item.protected, egui::Checkbox::new(&mut locked, "Lock"))
            .changed()
        {
            f.dispatch(
                Command::Lock {
                    id: item.id,
                    locked,
                },
                "Protection updated.",
            );
        }
        let mut favorite = item.favorite;
        if ui.checkbox(&mut favorite, "Favorite").changed() {
            f.dispatch(
                Command::Favorite {
                    id: item.id,
                    favorite,
                },
                "Favorite updated.",
            );
        }
        let sale = f.game.sale_preview(item.id);
        if ui
            .add_enabled(
                sale.is_ok(),
                egui::Button::new(format!(
                    "Review sale · {}",
                    f.game
                        .campaign
                        .item_sale_value(item.id)
                        .expect("owned equipment")
                )),
            )
            .on_disabled_hover_text(sale.err().unwrap_or_default())
            .clicked()
        {
            f.armory.pending_sale = Some(item.id);
        }
    });
    let definition = &content().items[item.definition];
    let npc_restricted = definition.npc_only
        && f.campaign.mode == Mode::Personal
        && !f.game.campaign.legacy_weapon_training;
    if definition.npc_only {
        muted(
            ui,
            if npc_restricted {
                "NPC issued gear"
            } else if f.campaign.mode == Mode::Personal {
                "NPC issued gear · legacy personal access retained"
            } else {
                "NPC issued gear"
            },
        );
    }
    if npc_restricted {
        muted(
            ui,
            f.game
                .campaign
                .weapon_access(f.campaign.mode, item.definition)
                .err()
                .unwrap_or_default(),
        );
    }
    if definition.slot == Slot::Ranged {
        let weapon = horde_core::weapons::WeaponSpec::from_item(definition);
        let trained = f
            .game
            .campaign
            .can_use_weapon(f.campaign.mode, item.definition);
        muted(
            ui,
            format!(
                "{} · {} · owned item #{}",
                weapon.class.name(),
                if f.campaign.mode == Mode::Npc {
                    "NPC Guns training"
                } else if npc_restricted {
                    "NPC issue only"
                } else if trained {
                    "proficient"
                } else {
                    "proficiency required"
                },
                item.id
            ),
        );
        if !trained && !npc_restricted && ui.button("Open weapon training").clicked() {
            let training = horde_core::skill_catalog::skills().nodes.iter().find(|n| n.effects.iter().any(|e| matches!(e, horde_core::skill_catalog::SkillEffect::WeaponTraining(class) if *class == weapon.class)));
            f.campaign.improvement_focus = training
                .map(|n| horde_core::campaign::RecommendationAction::LearnNode(n.id.clone()));
            f.open_preparation(Tab::Skills);
        }
    }
    item_sale_confirmation(ui, f);
    if reserved {
        muted(
            ui,
            "Reserved by an active expedition or guard. Recall that survivor before changing this item.",
        );
    }
}

fn sale_confirmation(ui: &mut egui::Ui, f: &mut UiCtx) {
    let quote = f.game.sell_extras_quote();
    if ui
        .add_enabled(
            quote > 0,
            egui::Button::new(format!("Sell unprotected extras · {quote} coins")),
        )
        .clicked()
    {
        f.armory.sell_confirm = true;
    }
    if f.armory.sell_confirm {
        ui.horizontal_wrapped(|ui| {
            ui.colored_label(
                AMBER,
                format!("Sell unused, unprotected extras for {quote} coins?"),
            );
            if ui.button("Confirm sale").clicked() {
                f.dispatch(
                    Command::SellUnprotected,
                    "Extras sold; equipped and preset items retained.",
                );
                f.armory.sell_confirm = false;
            }
            if ui.button("Cancel").clicked() {
                f.armory.sell_confirm = false;
            }
        });
    }
}

fn item_sale_confirmation(ui: &mut egui::Ui, f: &mut UiCtx) {
    if let Some(id) = f.armory.pending_sale {
        ui.add_space(8.0);
        match f.game.sale_preview(id) {
            Ok(quote) => {
                let name = f
                    .game
                    .campaign
                    .item(id)
                    .map(|item| content().items[item.definition].name.as_str())
                    .unwrap_or("Equipment");
                ui.colored_label(
                    AMBER,
                    format!("Sell {name} #{id} for {} coins?", quote.coins),
                );
                if !quote.affected_presets.is_empty() {
                    ui.label(format!(
                        "These presets will be incomplete: {}",
                        quote.affected_presets.join(", ")
                    ));
                }
                if !quote.affected_survivors.is_empty() {
                    ui.label(format!(
                        "These survivors will use protected starter equipment: {}",
                        quote.affected_survivors.join(", ")
                    ));
                }
                ui.horizontal(|ui| {
                    if ui.button("Confirm item sale").clicked() {
                        f.dispatch(
                            Command::SellItem(id),
                            "Item sold. Affected loadouts use protected equipment.",
                        );
                    }
                    if ui.button("Keep item").clicked() {
                        f.armory.pending_sale = None;
                    }
                });
            }
            Err(reason) => {
                ui.colored_label(RED, reason);
                if ui.button("Dismiss sale").clicked() {
                    f.armory.pending_sale = None;
                }
            }
        }
    }
}
