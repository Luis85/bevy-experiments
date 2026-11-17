//! Seeded headless exercises. Un-gated: the shipped `--smoke` command runs them.

use super::*;
/// Seeded, renderer-free full encounter exercise used by the desktop --smoke
/// command. The manual bot supplies ordinary geometric aiming inputs only.
pub fn smoke() -> Result<String, String> {
    smoke_seed(0x1A571167)
}
pub fn smoke_seed(seed: u64) -> Result<String, String> {
    let mut lines = Vec::new();
    for mode in [Mode::Personal, Mode::Npc] {
        let mut g = Game {
            seed,
            ..Game::default()
        };
        g.campaign.attempts = 1;
        if mode == Mode::Npc {
            g.campaign.legacy_features = true;
            g.campaign.npc.policy = Policy::BossFocus;
        }
        for level in 0..5 {
            let mut cleared = false;
            for retry in 0..6 {
                smoke_prepare(&mut g, mode)?;
                g.deploy(level, mode)?;
                if mode == Mode::Npc && g.campaign.blueprints[StructureKind::Trap] {
                    for lane in
                        construction_lane_order(g.attempt.as_ref().expect("deployed").lane_count)
                    {
                        if g.attempt.as_ref().is_some_and(|a| {
                            a.supply >= content().structure(StructureKind::Trap).supply
                        }) {
                            g.place(StructureKind::Trap, lane, 0)?;
                        }
                    }
                }
                g.set_paused(false);
                for _ in 0..60 * 600 {
                    if g.attempt.is_none() {
                        break;
                    }
                    let input = smoke_input(&g);
                    g.tick(input);
                }
                let r = g
                    .last_result
                    .as_ref()
                    .ok_or("Smoke encounter exceeded ten simulated minutes")?;
                lines.push(format!(
                    "{mode:?} stage{} attempt{} {:?} {:.1}s · {} kills · {} coins retained",
                    level + 1,
                    retry + 1,
                    r.outcome,
                    r.elapsed,
                    r.kills,
                    r.coins
                ));
                if r.outcome == Outcome::Victory {
                    cleared = true;
                    break;
                }
            }
            if !cleared {
                return Err(format!(
                    "No campaign clear within six earned-reward retries:\n{}",
                    lines.join("\n")
                ));
            }
        }
    }
    Ok(lines.join("\n"))
}
/// The desktop `--smoke` exercise: catalogue validation, an atomic save/resume
/// round trip, the fresh companion gate, a legacy autonomous run, then the
/// seeded [`smoke`] campaign. Moved from the frontend so it needs no field
/// access; the printed report is unchanged.
pub fn desktop_smoke() -> Result<String, String> {
    use crate::world::world;
    content().validate()?;
    world().validate()?;
    let mut game = Game::default();
    game.campaign
        .choose_survivor(crate::survivors::SurvivorId::Rook)?;
    game.campaign.choose_start_country("DEU")?;
    game.deploy_country_stage(0, Mode::Personal)?;
    game.attempt.as_mut().unwrap().paused = false;
    for _ in 0..180 {
        game.tick(Input::default());
    }
    let path = std::env::temp_dir().join(format!("lastlight-smoke-{}.json", std::process::id()));
    game.save(&path)?;
    let resumed = Game::load(&path)?;
    if serde_json::to_value(&game).map_err(|e| e.to_string())?
        != serde_json::to_value(&resumed).map_err(|e| e.to_string())?
    {
        return Err("Save changed simulation state".into());
    }
    std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    game.withdraw();
    if game.campaign.npc_available() {
        return Err("Fresh campaign unlocked the companion before its tree requirements".into());
    }
    let fresh = serde_json::to_value(&game).map_err(|e| e.to_string())?;
    if game.deploy_country_stage(0, Mode::Npc).is_ok()
        || serde_json::to_value(&game).map_err(|e| e.to_string())? != fresh
    {
        return Err("Locked companion deployment succeeded or mutated fresh state".into());
    }
    // Separately retain the old-save autonomous compatibility exercise. Fresh
    // progression above must not bypass the new authored companion unlock.
    game.campaign.legacy_features = true;
    game.deploy_country_stage(0, Mode::Npc)?;
    game.attempt.as_mut().unwrap().paused = false;
    for _ in 0..60 * 420 {
        game.tick(Input::default());
        if game.attempt.is_none() {
            break;
        }
    }
    let report = game
        .last_result
        .as_ref()
        .ok_or("NPC attempt did not terminate in seven simulated minutes")?;
    let baseline = format!(
        "PASS: {} country profiles; {} encounters; atomic resume identical; fresh companion gate is atomic; legacy autonomous {:?}: {} kills, {} coins in {:.1}s.\n{}",
        world().countries.len(),
        content().levels.len(),
        report.outcome,
        report.kills,
        report.coins,
        report.elapsed,
        report.cause
    );
    Ok(format!("{baseline}\n{}", smoke()?))
}
/// Legal earned-reward preparation used by deterministic campaign proofs.
pub fn smoke_prepare(g: &mut Game, mode: Mode) -> Result<(), String> {
    if mode == Mode::Npc
        && g.campaign.blueprint_available(StructureKind::Trap)
        && !g.campaign.blueprints[StructureKind::Trap]
        && g.campaign.coins >= g.campaign.blueprint_price(StructureKind::Trap)
    {
        g.campaign.purchase_blueprint(StructureKind::Trap)?;
    }
    if mode == Mode::Personal {
        for id in [
            "quick_reload",
            "rifle_training",
            "steady_hands",
            "rapid_cycle",
            "stopping_power",
            "piercing",
            "field_fitness",
            "second_wind",
            "brace",
        ] {
            let node = crate::skill_catalog::skills()
                .node(id)
                .expect("shipped smoke purchase route");
            if !g.campaign.has_node(id)
                && node.price <= g.campaign.coins
                && node.prerequisites.iter().all(|p| g.campaign.has_node(p))
            {
                g.campaign.purchase_node(id)?;
            }
        }
    }
    smoke_buy_gear(g, mode)?;
    if mode == Mode::Npc {
        while g.campaign.npc.unspent < 4
            && g.campaign.coins >= g.campaign.npc_training_price()
            && g.campaign.npc.awarded < g.campaign.allowance()
        {
            g.campaign.buy_training_point()?;
        }
        if g.campaign.npc.unspent >= 4 {
            g.campaign.train_npc(crate::campaign::NpcTraining::Guns)?;
        }
        if g.campaign.npc.guns >= 17 {
            // A trained defender facing a dense horde trades
            // optional Aim turns for more legal attack attempts.
            g.campaign.npc.prefer_aim = false;
            g.campaign.npc.allow_risk = true;
        }
    } else {
        while g.campaign.player.points > 0
            && g.campaign.player.body[crate::campaign::BodyAttribute::Vitality]
                < g.campaign.body_cap()
        {
            g.campaign
                .allocate_body(crate::campaign::BodyAttribute::Vitality)?;
        }
    }
    Ok(())
}
/// The earned shop half of [`smoke_prepare`]: buy and equip the best
/// affordable upgrade per slot. Shared with the pacing simulation.
pub(crate) fn smoke_buy_gear(g: &mut Game, mode: Mode) -> Result<(), String> {
    smoke_buy_gear_with(g, mode, |g, definition| {
        let id = g.campaign.buy_item(definition)?;
        g.campaign.equip(mode, id)
    })
}
/// [`smoke_buy_gear`]'s choice, with `buy` buying and equipping the
/// catalogue item at index `definition` for `mode`. The pacing strategy bot
/// (W2-43) passes `BuyItem` and `Equip` commands.
pub(crate) fn smoke_buy_gear_with(
    g: &mut Game,
    mode: Mode,
    mut buy: impl FnMut(&mut Game, usize) -> Result<(), String>,
) -> Result<(), String> {
    let mut market: Vec<usize> = content()
        .items
        .iter()
        .enumerate()
        .filter(|(definition, item)| {
            item.purchasable()
                && item.tier <= g.campaign.unlocked()
                && g.campaign
                    .buy_price(*definition)
                    .is_ok_and(|price| price <= g.campaign.coins)
                && (mode == Mode::Npc || g.campaign.can_use_weapon(mode, *definition))
        })
        .map(|(n, _)| n)
        .collect();
    market.sort_by_key(|n| {
        (
            match content().items[*n].slot {
                crate::content::Slot::Ranged => 0,
                crate::content::Slot::Outfit => 1,
                crate::content::Slot::Accessory => 2,
                crate::content::Slot::Melee => 3,
            },
            std::cmp::Reverse(content().items[*n].tier),
            std::cmp::Reverse(
                if mode == Mode::Npc && content().items[*n].slot == crate::content::Slot::Ranged {
                    // Compare guaranteed catalogue profiles within a
                    // tier, including sustained magazine capacity.
                    let profile = &content().items[*n].npc;
                    i64::from(profile.damage_dice) * 28
                        + i64::from(profile.damage_add) * 8
                        + i64::from(profile.accuracy) * 3
                        + i64::from(profile.magazine)
                } else {
                    0
                },
            ),
        )
    });
    let mut bought = [false; 4];
    for n in market {
        let def = &content().items[n];
        let slot = def.slot.index();
        if !bought[slot]
            && (g.campaign.equipped(mode, def.slot).tier < def.tier
                || (g.campaign.equipped(mode, def.slot).tier == def.tier
                    && g.campaign
                        .item(if mode == Mode::Personal {
                            g.campaign.player.loadout[slot]
                        } else {
                            g.campaign.npc.loadout[slot]
                        })
                        .is_some_and(|item| item.level < g.campaign.shop_item_level())))
            && g.campaign
                .buy_price(n)
                .is_ok_and(|price| price <= g.campaign.coins)
        {
            buy(g, n)?;
            bought[slot] = true;
        }
    }
    Ok(())
}
pub fn smoke_input(g: &Game) -> Input {
    let Some(a) = &g.attempt else {
        return Input::default();
    };
    let target = a
        .enemies
        .iter()
        .filter(|e| e.in_view())
        .min_by(|e, f| e.x.total_cmp(&f.x).then(e.id.cmp(&f.id)));
    let Some(e) = target else {
        return Input::default();
    };
    Input {
        aim: [e.x, e.y()],
        fire: true,
        melee: e.contact() && a.stamina >= 25.0,
        heal: a.hp < a.max_hp / 2,
        brace: a.hp < a.max_hp / 2,
        ..Input::default()
    }
}
