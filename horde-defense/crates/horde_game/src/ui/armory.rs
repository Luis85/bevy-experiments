//! Armory presentation. Game changes are queued typed intents.
use super::{Tab, UiCtx, chrome, coins::grouped, mode_selector, muted};
use crate::art::{AMBER, GREEN, INK, MUTED, RED, TEXT};
use bevy_egui::egui::{self, Align, Color32, Layout, Rect, RichText, Sense, Stroke, vec2};
use horde_core::{
    campaign::{SaleBlock, SaleResolution, UpgradeFocus},
    command::Command,
    content::{LOOT_ONLY_REASON, Mode, Slot, content},
};
mod auto_sell;
mod view;
use view::{ArmoryOffer, OfferTraining};
/// Right-aligned item-row column holding `Equip` or the `Equipped` status.
const STATUS_COLUMN_WIDTH: f32 = 96.0;
/// Shown instead of a before/after comparison for an equipped selection.
pub(super) const CURRENTLY_EQUIPPED: &str = "Currently equipped";
/// Hover reason on a disabled purchase.
const NEED_COINS_REASON: &str = "Not enough coins yet. Earn more in your next defense.";
/// Hover reason on the disabled `Equip selected` action.
const ALREADY_EQUIPPED_REASON: &str =
    "Already equipped in this loadout. Select another item to compare or equip it.";
fn mode_loadout_contains(f: &UiCtx, id: u64) -> bool {
    if f.campaign.mode == Mode::Personal {
        f.game.campaign().player.loadout.contains(&id)
    } else {
        f.game.campaign().npc.loadout.contains(&id)
    }
}
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

pub(super) fn loot_receipt(
    ui: &mut egui::Ui,
    receipt: &horde_core::quality::LootEvent,
    settings: &crate::settings::Settings,
) {
    let text = format!(
        "{} · {} · Lv {}",
        receipt.quality.name(),
        receipt.name,
        receipt.level
    );
    chrome::with_quality_shape(ui, receipt.quality, settings, |ui| {
        ui.add(
            egui::Label::new(
                RichText::new(&text)
                    .color(chrome::quality_colour(receipt.quality, settings.palette)),
            )
            .truncate(),
        )
        .on_hover_text(format!(
            "{text} · owned #{} · already banked",
            receipt.item_id
        ));
    });
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
    let colour = chrome::quality_colour(quality, f.settings.palette);
    ui.painter().rect_stroke(
        rect,
        0.0,
        Stroke::new(1.5, colour),
        egui::StrokeKind::Inside,
    );
    if f.settings.quality_markers.shape() {
        // The frame's colour alone carried the quality: its shape sits in
        // the corner on an ink backing, clear of the icon.
        let side = chrome::QUALITY_SHAPE_SIDE;
        let corner = Rect::from_min_size(rect.right_bottom() - vec2(side, side), vec2(side, side));
        ui.painter().rect_filled(corner, 0.0, INK);
        chrome::paint_quality_shape(ui.painter(), quality, corner.center(), side - 2.0, colour);
    }
}
fn inventory_comparison(ui: &mut egui::Ui, f: &UiCtx, id: u64) {
    let Some(item) = f.game.campaign().item(id) else {
        return;
    };
    let definition = item.definition();
    let quality = f.game.campaign().item_quality(id).expect("owned equipment");
    let Ok(comparison) = f.game.campaign().compare_item(f.campaign.mode, id) else {
        return;
    };
    let before = &comparison.before;
    let after = &comparison.after;
    ui.horizontal(|ui| {
        slot_art(ui, f, item.definition_index(), quality);
        ui.vertical(|ui| {
            ui.label(
                chrome::quality_item_text(
                    format!("{} #{} · Lv {}", definition.name, id, item.level),
                    quality,
                    f.settings,
                )
                .strong(),
            );
            // A loot-only definition is never unlocked or bought: core's
            // reason stands where the unlock tier would.
            let source = if definition.loot_only {
                LOOT_ONLY_REASON.to_owned()
            } else {
                format!("unlock tier {}", definition.tier)
            };
            muted(
                ui,
                format!(
                    "{} quality · {} · {source}",
                    quality.name(),
                    definition.slot.label()
                ),
            );
        });
    });
    // An equipped selection would repeat identical before/after columns.
    let equipped = mode_loadout_contains(f, id);
    if equipped {
        ui.colored_label(GREEN, CURRENTLY_EQUIPPED);
        muted(
            ui,
            "These are your current loadout values. Select another item to compare it.",
        );
    }
    egui::Grid::new(("item_comparison", id))
        .num_columns(if equipped { 2 } else { 3 })
        .spacing([18.0, 6.0])
        .show(ui, |ui| {
            ui.label("Loadout effect");
            if equipped {
                ui.strong("Current loadout");
            } else {
                ui.strong("Equipped");
                ui.strong("With this item");
            }
            ui.end_row();
            let mut row = |label: &str, old: String, new: String, better: bool, worse: bool| {
                ui.label(label);
                ui.label(old);
                if equipped {
                    ui.end_row();
                    return;
                }
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
                    format!(
                        "{}d{:+} {}",
                        before.dice,
                        before.adds,
                        before.damage_kind.label()
                    ),
                    format!(
                        "{}d{:+} {}",
                        after.dice,
                        after.adds,
                        after.damage_kind.label()
                    ),
                    after.expected_basic_damage > before.expected_basic_damage,
                    after.expected_basic_damage < before.expected_basic_damage,
                );
                row(
                    "Melee profile",
                    format!(
                        "{}d{:+} {}",
                        before.melee_dice,
                        before.melee_adds,
                        before.melee_kind.label()
                    ),
                    format!(
                        "{}d{:+} {}",
                        after.melee_dice,
                        after.melee_adds,
                        after.melee_kind.label()
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
                "{} · {} per shot · {:.0}° spread",
                weapon.class.name(),
                chrome::count_noun(weapon.pellets as usize, "projectile", "projectiles"),
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
/// The page purpose, kept off the fold and shown on the title's hover.
const ARMORY_PURPOSE: &str = "Choose equipment for the next defense. Deployed and guarding survivors reserve their unique items.";
/// The gear-level line: a short form for the header row, the full one on hover.
fn gear_level_line(f: &UiCtx) -> (String, String) {
    let campaign = f.game.campaign();
    let (rank, level) = (campaign.progression_rank(), campaign.shop_item_level());
    match campaign.next_gear_level_rank() {
        Some(next) => (
            format!("New gear Lv {level} · next at rank {next}"),
            format!(
                "New equipment: Lv {level} · threat rank {rank} · next gear level at rank {next}"
            ),
        ),
        None => (
            format!("New gear Lv {level} · maximum"),
            format!("New equipment: Lv {level} · maximum authored item level reached"),
        ),
    }
}
pub(super) fn armory(ui: &mut egui::Ui, f: &mut UiCtx) {
    // One header row (title, defender, gear level) and one filter row, so the
    // inventory starts above the fold at the minimum scaled window.
    ui.horizontal_wrapped(|ui| {
        // Labels and pickers share the row's centre line with the title.
        chrome::begin_filter_row(ui);
        ui.label(RichText::new("Equipment").font(egui::FontId::new(
            28.0,
            egui::FontFamily::Name("display".into()),
        )))
        .on_hover_text(ARMORY_PURPOSE);
        ui.add_space(16.0);
        mode_selector(ui, f);
        ui.add_space(16.0);
        let (short, detail) = gear_level_line(f);
        ui.label(RichText::new(short).color(MUTED))
            .on_hover_text(detail);
    });
    ui.add_space(4.0);
    let view = view::view_model(
        f.game,
        f.campaign.mode,
        f.campaign.improvement_focus.as_ref(),
    );
    if let Some(id) = view.equip_focus {
        f.armory.selected_item = Some(id);
    }
    if let Some(offer) = &view.recommended {
        ui.push_id("recommended_equipment", |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.strong(format!("Recommended: {}", offer.item.name))
                    .on_hover_text(
                        "Purchase this item for your next defense, then choose whether to equip it.",
                    );
                if ui
                    .add_enabled(
                        offer.tier_unlocked && offer.affordable,
                        egui::Button::new(format!("Buy · {} coins", grouped(offer.price))),
                    )
                    .on_disabled_hover_text(if offer.tier_unlocked {
                        NEED_COINS_REASON
                    } else {
                        "This quality tier unlocks with later victories."
                    })
                    .clicked()
                {
                    f.dispatch(
                        Command::BuyItem(offer.id.clone()),
                        "Equipment added. Select it below to compare before equipping.",
                    );
                }
            });
        });
    }
    // The count sits in the filter row; a filter changed this frame
    // re-filters before the list renders.
    let filters = (
        f.armory.search.clone(),
        f.armory.slot,
        f.armory.quality,
        f.armory.sort,
    );
    let mut indices = filtered_indices(f);
    ui.horizontal_wrapped(|ui| {
        let row = chrome::begin_filter_row(ui);
        ui.add(
            egui::TextEdit::singleline(&mut f.armory.search)
                .hint_text("Search equipment")
                .desired_width(160.0)
                .min_size(vec2(160.0, row))
                .vertical_align(Align::Center),
        );
        egui::ComboBox::from_id_salt("armory_slot")
            .selected_text(f.armory.slot.map_or("All slots", Slot::label))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut f.armory.slot, None, "All slots");
                for slot in Slot::ALL {
                    ui.selectable_value(&mut f.armory.slot, Some(slot), slot.label());
                }
            });
        egui::ComboBox::from_id_salt("armory_quality")
            .selected_text(f.armory.quality.map_or_else(
                || RichText::new("All qualities"),
                |quality| {
                    RichText::new(quality.name())
                        .color(chrome::quality_colour(quality, f.settings.palette))
                },
            ))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut f.armory.quality, None, "All qualities");
                for quality in horde_core::quality::Quality::ALL {
                    ui.selectable_value(
                        &mut f.armory.quality,
                        Some(quality),
                        RichText::new(quality.name())
                            .color(chrome::quality_colour(quality, f.settings.palette)),
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
        ui.label(
            RichText::new(format!(
                "{} of {} items",
                indices.len(),
                f.game.campaign().items.len()
            ))
            .color(MUTED),
        )
        .on_hover_text("Storage has no item limit.");
    });
    if filters
        != (
            f.armory.search.clone(),
            f.armory.slot,
            f.armory.quality,
            f.armory.sort,
        )
    {
        indices = filtered_indices(f);
    }
    ui.add_space(4.0);

    // Selection is view state only. Never change equipment implicitly.
    if f.armory
        .selected_item
        .is_none_or(|id| f.game.campaign().item(id).is_none())
    {
        f.armory.selected_item = indices
            .first()
            .map(|index| f.game.campaign().items[*index].id);
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
        // Beside the comparison, the list may use the page height.
        let viewport = (ui.clip_rect().bottom() - ui.cursor().top() - 8.0).clamp(150.0, 520.0);
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                vec2(list_width, viewport),
                Layout::top_down(Align::Min),
                |ui| inventory_list(ui, f, &indices, viewport),
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
        let viewport = (ui.clip_rect().height() * 0.46).clamp(150.0, 330.0);
        inventory_list(ui, f, &indices, viewport);
        if let Some(id) = f.armory.selected_item {
            let name = f
                .game
                .campaign()
                .item(id)
                .map(|item| item.definition().name.as_str())
                .unwrap_or("Item");
            let quality = f.game.campaign().item_quality(id).expect("owned item");
            ui.label(
                RichText::new(format!("Selected: {name} #{id} · {}", quality.name()))
                    .strong()
                    .color(chrome::quality_colour(quality, f.settings.palette)),
            );
            selected_actions(ui, f);
            egui::CollapsingHeader::new("Compare complete loadout")
                .id_salt("compact_item_comparison")
                .show(ui, |ui| inventory_comparison(ui, f, id));
        }
    }
    chrome::section(ui, "Manage equipment");
    sale_confirmation(ui, f);
    auto_sell::panel(ui, f);
    egui::CollapsingHeader::new("Loadout presets").show(ui, |ui| {
        muted(ui, "Save references to your current equipment. Referenced items are retained by bulk sales.");
        ui.horizontal_wrapped(|ui| {
            ui.add(egui::TextEdit::singleline(&mut f.armory.preset_name).hint_text("Preset name").desired_width(180.0));
            if ui.button("Save loadout preset").clicked() { f.dispatch(Command::SavePreset { name: f.armory.preset_name.clone(), mode: f.campaign.mode }, "Loadout saved by reference."); }
            for (index, preset) in f.game.campaign().presets.clone().iter().enumerate() {
                if ui.button(format!("Use {}", preset.name)).clicked() { f.dispatch(Command::ApplyPreset { index, mode: f.campaign.mode }, "Loadout equipped."); }
                if ui.button(format!("Delete {}", preset.name)).on_hover_text("Removes this preset only; its items stay in the armory.").clicked() { f.dispatch(Command::DeletePreset { index }, "Preset deleted."); }
            }
        });
    });
    egui::CollapsingHeader::new("Quartermaster · guaranteed equipment").open(view.recommended.as_ref().map(|_| true)).show(ui, |ui| {
        muted(ui, "Progression never requires a rare drop. Purchases go to the armory; equip them separately.");
    for ArmoryOffer { preview: offer, weapon } in &view.offers {
        let item = &offer.item;
        ui.horizontal(|ui| {
            slot_art(ui, f, offer.definition, item.base_quality);
            ui.label(RichText::new(format!("{} · {} · {} · Lv {}", item.name, item.base_quality.name(), item.slot.label(), offer.level)).color(chrome::quality_colour(item.base_quality, f.settings.palette))).on_hover_text(format!("Personal: {} damage · {} armor · {} health. NPC: {}d{:+} · {} armor. Stats include this offered item level.", item.personal.damage, item.personal.armor, item.personal.health, item.npc.damage_dice, item.npc.damage_add, item.npc.armor));
            if let Some((class, training)) = weapon {
                ui.label(format!("{} · {}", class.name(), match training { OfferTraining::NpcIssued => "NPC issued gear", OfferTraining::Trained => "trained", OfferTraining::TrainingRequired => "training required" }));
            }
            if ui
                .add_enabled(
                    offer.affordable,
                    egui::Button::new(format!("Buy · {} coins", grouped(offer.price))),
                )
                .on_disabled_hover_text(NEED_COINS_REASON)
                .clicked()
            {
                f.dispatch(
                    Command::BuyItem(offer.id.clone()),
                    "Equipment added. Select it above to compare before equipping.",
                );
            }
        });
    }
    });
}

/// Owned item indices matching the armory filters, in the chosen order.
fn filtered_indices(f: &UiCtx) -> Vec<usize> {
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
        .campaign()
        .items
        .iter()
        .enumerate()
        .filter(|(_, item)| {
            matches[item.definition_index()]
                && f.armory
                    .slot
                    .is_none_or(|slot| item.definition().slot == slot)
                && f.armory.quality.is_none_or(|quality| {
                    f.game.campaign().item_quality(item.id).ok() == Some(quality)
                })
        })
        .map(|(index, _)| index)
        .collect();
    indices.sort_by(|&a, &b| {
        let a = &f.game.campaign().items[a];
        let b = &f.game.campaign().items[b];
        match f.armory.sort {
            ArmorySort::Newest => b.id.cmp(&a.id),
            ArmorySort::Name => a
                .definition()
                .name
                .cmp(&b.definition().name)
                .then(a.id.cmp(&b.id)),
            ArmorySort::Level => b.level.cmp(&a.level).then(b.id.cmp(&a.id)),
            ArmorySort::Quality => f
                .game
                .campaign()
                .item_quality(b.id)
                .expect("owned item")
                .cmp(&f.game.campaign().item_quality(a.id).expect("owned item"))
                .then(b.level.cmp(&a.level))
                .then(b.id.cmp(&a.id)),
            ArmorySort::Tier => b
                .definition()
                .tier
                .cmp(&a.definition().tier)
                .then(a.id.cmp(&b.id)),
        }
    });
    indices
}

fn inventory_list(ui: &mut egui::Ui, f: &mut UiCtx, indices: &[usize], viewport: f32) {
    egui::ScrollArea::vertical()
        .id_salt("armory")
        .max_height(viewport)
        .show_rows(ui, 72.0, indices.len(), |ui, range| {
            for index in range {
                let item = f.game.campaign().items[indices[index]].clone();
                let definition = item.definition();
                let quality = f
                    .game
                    .campaign()
                    .item_quality(item.id)
                    .expect("owned equipment");
                let equipped = mode_loadout_contains(f, item.id);
                let reserved = f.game.campaign().item_reserved(item.id);
                ui.push_id(item.id, |ui| {
                    ui.horizontal_top(|ui| {
                        slot_art(ui, f, item.definition_index(), quality);
                        // A fixed-width name column keeps every row's status
                        // column at the same x, whatever the item name length.
                        let name_width = (ui.available_width()
                            - STATUS_COLUMN_WIDTH
                            - ui.spacing().item_spacing.x)
                            .max(150.0);
                        ui.allocate_ui_with_layout(
                            vec2(name_width, 62.0),
                            Layout::top_down(Align::Min),
                            |ui| {
                                ui.set_width(name_width);
                                // Rarity text retains contrast when selected; the
                                // ordinary amber selection fill muddies blue/purple.
                                ui.visuals_mut().selection.bg_fill = INK;
                                if ui
                                    .selectable_label(
                                        f.armory.selected_item == Some(item.id),
                                        chrome::quality_item_text(
                                            format!(
                                                "{} #{} · Lv {}",
                                                definition.name, item.id, item.level
                                            ),
                                            quality,
                                            f.settings,
                                        ),
                                    )
                                    .clicked()
                                {
                                    f.armory.selected_item = Some(item.id);
                                }
                                muted(
                                    ui,
                                    format!(
                                        "{} · {} · {}{}{}{}",
                                        quality.name(),
                                        definition.slot.label(),
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
                        ui.allocate_ui_with_layout(
                            vec2(STATUS_COLUMN_WIDTH, 36.0),
                            Layout::right_to_left(Align::Center),
                            |ui| {
                                status_cell(
                                    ui,
                                    f,
                                    item.id,
                                    item.definition_index(),
                                    equipped,
                                    reserved,
                                )
                            },
                        );
                    });
                });
            }
        });
}

/// One fixed-size slot per row, so `Equip` and `Equipped` share a centre line.
fn status_cell(
    ui: &mut egui::Ui,
    f: &mut UiCtx,
    id: u64,
    definition: usize,
    equipped: bool,
    reserved: bool,
) {
    let size = vec2(STATUS_COLUMN_WIDTH - 8.0, 28.0);
    if equipped {
        ui.add_sized(
            size,
            egui::Label::new(RichText::new("Equipped").color(GREEN)),
        );
        return;
    }
    let usable = !reserved
        && f.game
            .campaign()
            .can_use_weapon(f.campaign.mode, definition);
    let response = ui
        .add_enabled_ui(usable, |ui| ui.add_sized(size, egui::Button::new("Equip")))
        .inner
        .on_disabled_hover_text(if reserved {
            "Reserved by a deployed survivor or guard."
        } else {
            f.game
                .campaign()
                .weapon_access(f.campaign.mode, definition)
                .err()
                .unwrap_or_default()
        });
    if response.clicked() {
        f.dispatch(
            Command::Equip {
                mode: f.campaign.mode,
                id,
            },
            "Equipment changed.",
        );
    }
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
        .and_then(|id| f.game.campaign().item(id))
        .cloned()
    else {
        return;
    };
    let equipped = mode_loadout_contains(f, item.id);
    let reserved = f.game.campaign().item_reserved(item.id);
    let sale = f.game.sale_eligibility(item.id);
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(
                !reserved
                    && !equipped
                    && f.game
                        .campaign()
                        .can_use_weapon(f.campaign.mode, item.definition_index()),
                egui::Button::new("Equip selected"),
            )
            .on_disabled_hover_text(if equipped {
                ALREADY_EQUIPPED_REASON
            } else if reserved {
                "Reserved by a deployed survivor or guard."
            } else {
                f.game
                    .campaign()
                    .weapon_access(f.campaign.mode, item.definition_index())
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
            .on_disabled_hover_text("Starter equipment is always protected.")
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
        // Strict SellItem (ruleset 2): a blocked item cannot be reviewed for
        // sale, and the disabled button says why in the core's words.
        if ui
            .add_enabled(
                sale.is_ok(),
                egui::Button::new(format!(
                    "Review sale · {}",
                    f.game
                        .campaign()
                        .item_sale_value(item.id)
                        .expect("owned equipment")
                )),
            )
            .on_disabled_hover_text(
                sale.as_ref()
                    .err()
                    .map(SaleBlock::label)
                    .unwrap_or_default(),
            )
            .clicked()
        {
            f.armory.pending_sale = Some(item.id);
        }
    });
    if let Err(block) = &sale {
        blocked_sale(ui, f, item.id, block);
    }
    let definition = item.definition();
    let npc_restricted = definition.npc_only
        && f.campaign.mode == Mode::Personal
        && !f.game.campaign().legacy_weapon_training;
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
                .campaign()
                .weapon_access(f.campaign.mode, item.definition_index())
                .err()
                .unwrap_or_default(),
        );
    }
    if definition.slot == Slot::Ranged {
        let weapon = horde_core::weapons::WeaponSpec::from_item(definition);
        let trained = f
            .game
            .campaign()
            .can_use_weapon(f.campaign.mode, item.definition_index());
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
            f.campaign.improvement_focus = f
                .game
                .campaign()
                .recommendation_fallback(UpgradeFocus::WeaponTraining(weapon.class))
                .map(|r| r.action);
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

/// Why the selected item cannot be sold, in core's words
/// (`SaleBlock::label`), and the shortcut core names for that guard
/// (`Game::sale_resolution`): its command, or the way to Operations for a
/// recruit's gear. A guard with no way out shows the reason alone.
fn blocked_sale(ui: &mut egui::Ui, f: &mut UiCtx, id: u64, block: &SaleBlock) {
    ui.horizontal_wrapped(|ui| {
        muted(ui, format!("Sale blocked: {}", block.label()));
        let (label, hover, done) = match f.game.sale_resolution(id) {
            SaleResolution::None => return,
            SaleResolution::OpenRecruit { .. } => {
                if f.operations_available()
                    && ui
                        .button("Operations ›")
                        .on_hover_text("A recruit's equipment is changed on Operations.")
                        .clicked()
                {
                    f.open_preparation(Tab::Operations);
                }
                return;
            }
            SaleResolution::Unlock => (
                "Unlock item".to_owned(),
                "Unlocked items can be sold.",
                "Item unlocked.",
            ),
            SaleResolution::Unfavourite => (
                "Remove favorite mark".to_owned(),
                "Items without the mark can be sold.",
                "Favorite mark removed.",
            ),
            SaleResolution::EquipStarter { mode, slot } => (
                match mode {
                    Mode::Personal => format!("Equip starter {}", slot.label()),
                    Mode::Npc => format!(
                        "Equip {}'s starter {}",
                        f.game.campaign().npc.name,
                        slot.label()
                    ),
                },
                "Puts the protected starter back on, so this item is free.",
                "Starter equipped.",
            ),
            SaleResolution::OpenPreset { index } => (
                format!(
                    "Delete preset {}",
                    f.game
                        .campaign()
                        .presets
                        .get(index)
                        .map_or("", |preset| preset.name.as_str())
                ),
                "Removes this preset only; its items stay in the armory.",
                "Preset deleted.",
            ),
        };
        if let Some(command) = f.game.sale_resolution_command(id)
            && ui.button(label).on_hover_text(hover).clicked()
        {
            f.dispatch(command, done);
        }
    });
}

fn sale_confirmation(ui: &mut egui::Ui, f: &mut UiCtx) {
    let quote = f.game.sell_extras_quote();
    if ui
        .add_enabled(
            quote > 0,
            egui::Button::new(format!(
                "Sell unprotected extras · {} coins",
                grouped(quote)
            )),
        )
        .on_disabled_hover_text(
            "Nothing to sell. Equipped, protected, locked, favorite and preset items are kept, and sales wait while a defender is deployed.",
        )
        .clicked()
    {
        f.armory.sell_confirm = true;
    }
    if f.armory.sell_confirm {
        ui.horizontal_wrapped(|ui| {
            ui.colored_label(
                AMBER,
                format!(
                    "Sell unused, unprotected extras for {} coins?",
                    grouped(quote)
                ),
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
        match f.game.sale_eligibility(id) {
            Err(block) => {
                ui.colored_label(RED, block.label());
                if ui.button("Dismiss sale").clicked() {
                    f.armory.pending_sale = None;
                }
            }
            Ok(quote) => {
                let name = f
                    .game
                    .campaign()
                    .item(id)
                    .map(|item| item.definition().name.as_str())
                    .unwrap_or("Equipment");
                ui.colored_label(
                    AMBER,
                    format!("Sell {name} #{id} for {} coins?", grouped(quote.coins)),
                );
                ui.horizontal(|ui| {
                    if ui.button("Confirm item sale").clicked() {
                        f.dispatch(Command::SellItem(id), "Item sold.");
                    }
                    if ui.button("Keep item").clicked() {
                        f.armory.pending_sale = None;
                    }
                });
            }
        }
    }
}
