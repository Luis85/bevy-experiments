//! Perception-scoped economic opportunities and the actor's own life/diary state.
use super::{VillageAct, catalog, economy as econ, model::*};
use bevy_ecs::prelude::*;
use npc_framework::agent_io::observe::known_targets;
use npc_framework::agent_io::{
    ActionPlan, ActionRequest, Affordance, AgentCtx, Rejection, Section,
};
use npc_framework::interaction::items::{self, entity_of};
use npc_framework::interaction::{CarriedBy, Item};
use npc_framework::prelude::*;
use serde_json::json;

pub fn verb_gate(world: &World, e: Entity, verb: &str) -> Option<String> {
    let v = world.get::<Vitality>(e)?;
    if v.died_at.is_some() {
        return (verb != "new_life").then(|| {
            "this actor is permanently dead; begin a new life in the same village".into()
        });
    }
    let person = world.get::<Villager>(e)?;
    if person.resting_until > world.resource::<SimClock>().tick
        && !matches!(verb, "rest" | "read_diary" | "write_diary" | "stop_work")
    {
        return Some(format!("resting until tick {}", person.resting_until));
    }
    None
}
pub fn observe(world: &World, ctx: &AgentCtx) -> Option<Section> {
    let person = world.get::<Villager>(ctx.entity)?;
    let vital = world.get::<Vitality>(ctx.entity)?;
    let coins = world.get::<Wallet>(ctx.entity).map_or(0, |w| w.coins);
    let (day, season, night) = calendar(world);
    let found:Vec<_>=items::carried_items(world,ctx.entity).into_iter().filter_map(|id|{
        let e=entity_of(world,id)?;let book=world.get::<DroppedDiary>(e)?;
        Some(json!({"id":id,"author":book.author_name,"life":book.life,"died_at":book.died_at,"published":book.sealed,"page_count":book.pages.len()}))
    }).collect();
    let known = known_targets(world, ctx);
    let mut market = Vec::new();
    for (name, id) in &world.get_resource::<crate::TestbedIds>()?.names {
        if known.as_ref().is_none_or(|k| !k.contains(id)) {
            continue;
        }
        let Some(e) = entity_of(world, *id) else {
            continue;
        };
        let Some(v) = world.get::<Villager>(e) else {
            continue;
        };
        if v.role == Role::Visitor
            || !living(world, e)
            || !econ::near(world, ctx.entity, e)
            || name != v.role.person()
        {
            continue;
        }
        let stock:Vec<_>=catalog::outputs(v.role).iter().map(|kind|json!({"kind":kind,"quantity":econ::count(world,e,kind),"price":catalog::price(kind)})).collect();
        market.push(json!({"id":id,"name":v.name,"role":v.role,"stock":stock,"bed_price":if v.role==Role::Innkeeper{Some(3)}else{None}}));
    }
    let value = json!({"name":person.name,"life":person.life,"alive":vital.died_at.is_none(),"health":vital.health,"died_at":vital.died_at,
        "coins":coins,"needs":{"hunger":econ::need(world,ctx.entity,"Hunger"),"thirst":econ::need(world,ctx.entity,"Thirst"),"energy":econ::need(world,ctx.entity,"Energy")},
        "resting_until":person.resting_until,"exhaustion_episodes":vital.exhaustion_ticks,
        "calendar":{"day":day+1,"season":season,"night":night,"days_per_season":DAYS_PER_SEASON},"nearby_market":market,"found_diaries":found});
    let mut section = Section::new(value).line(format!(
        "{} — life {}, {} coins, health {}/100000. Day {}, {:?}, {}.",
        person.name,
        person.life,
        coins,
        vital.health,
        day + 1,
        season,
        if night { "night" } else { "day" }
    ));
    if vital.died_at.is_some() {
        section=section.line("You are dead permanently. Your diary and possessions remain in this world. new_life creates a new actor; enter_world again after its arrival.");
    } else {
        section=section.line("Gather fallen wood or grain, or craft at village workstations, then sell to villagers who need the goods. Buy bread and clean water; eat/drink from your pack. A bed at the inn costs 3 coins. Water around the island cannot be crossed.")
        .line("NPC shops have finite stock and money. Merchant surplus circulates through a finite community fund; visiting actors receive no stipend. Winter reduces farming supply.")
        .line("Your Diary becomes a readable dropped item on permanent death. Memory, Proposals, map notes and credentials stay private. A successor is a different actor in the same continuing world.");
    }
    Some(section)
}
fn checked(mut a: Affordance, check: Result<(), String>) -> Affordance {
    if let Err(why) = check {
        a.ok = false;
        a.why_not = Some(why);
    }
    a
}
pub fn affordances(world: &World, ctx: &AgentCtx) -> Vec<Affordance> {
    if world.get::<Villager>(ctx.entity).is_none() {
        return Vec::new();
    }
    if !living(world, ctx.entity) {
        let cmd = VillageAct::new(ctx.unit, "new_life");
        return vec![checked(
            Affordance::new(
                "new_life",
                "Join this persistent village as a new actor; the dead actor stays dead. Re-enter the world afterward.",
            ),
            cmd.check(world),
        )];
    }
    let mut out = Vec::new();
    if world.get::<Gathering>(ctx.entity).is_some()
        || world
            .get::<npc_framework::crafting::Crafting>(ctx.entity)
            .is_some()
        || world
            .get::<Villager>(ctx.entity)
            .is_some_and(|v| v.resting_until > world.resource::<SimClock>().tick)
    {
        out.push(Affordance::new(
            "stop_work",
            "Stop gathering, crafting or resting",
        ));
    }
    for (verb, label) in [
        ("eat", "Eat one bread from your pack"),
        ("drink", "Drink one clean water from your pack"),
    ] {
        let cmd = VillageAct::new(ctx.unit, verb);
        out.push(checked(Affordance::new(verb, label), cmd.check(world)));
    }
    out.push(
        Affordance::new(
            "write_diary",
            "Write a physical diary page (others can read it after your death)",
        )
        .arg("title", "one line, up to 200 characters")
        .arg("body", "up to 8192 bytes"),
    );
    let known = known_targets(world, ctx).unwrap_or_default();
    for id in known {
        let Some(e) = entity_of(world, id) else {
            continue;
        };
        if let Some(v) = world.get::<Villager>(e)
            && v.role != Role::Visitor
            && living(world, e)
        {
            let target = |verb: &str| {
                let mut cmd = VillageAct::new(ctx.unit, verb);
                cmd.target = Some(id);
                cmd
            };
            out.push(checked(
                Affordance::new("talk", format!("Talk to {}, {}", v.name, v.role.name()))
                    .target(id),
                target("talk").check(world),
            ));
            let offered = catalog::outputs(v.role);
            if !offered.is_empty() {
                let a = Affordance::new(
                    "buy",
                    format!(
                        "Buy from {} ({})",
                        v.name,
                        offered
                            .iter()
                            .map(|k| format!("{k}: {} coins", catalog::price(k).unwrap_or(0)))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                )
                .target(id)
                .arg("kind", &offered.join(", "))
                .arg("quantity", "1..=20; default 1");
                let possible = offered.iter().any(|k| {
                    let mut c = target("buy");
                    c.kind = (*k).into();
                    c.check(world).is_ok()
                });
                out.push(checked(
                    a,
                    if possible {
                        Ok(())
                    } else {
                        Err("move close and check stock, coins and carrying capacity".into())
                    },
                ));
            }
            let wanted: Vec<_> = econ::free_items(world, ctx.entity)
                .iter()
                .map(|(_, i)| i.kind.clone())
                .filter(|k| catalog::wants(v.role, k))
                .collect();
            let possible = wanted.iter().any(|kind| {
                let mut c = target("sell");
                c.kind = kind.clone();
                c.check(world).is_ok()
            });
            out.push(checked(
                Affordance::new("sell", format!("Sell useful goods to {}", v.name))
                    .target(id)
                    .arg("kind", "a good in your pack this shop needs")
                    .arg("quantity", "1..=20; default 1"),
                if possible {
                    Ok(())
                } else {
                    Err("needs a useful good, shop funds/space and reach".into())
                },
            ));
            if v.role == Role::Innkeeper {
                out.push(checked(
                    Affordance::new("rest", "Rent a bed: 3 coins, 300 ticks").target(id),
                    target("rest").check(world),
                ));
            }
            if items::carried_items(world, e).into_iter().any(|id| {
                entity_of(world, id).is_some_and(|e| world.get::<DroppedDiary>(e).is_some())
            }) {
                out.push(checked(
                    Affordance::new("ask_diary", format!("Ask {} for a recovered diary", v.name))
                        .target(id),
                    target("ask_diary").check(world),
                ));
            }
        }
        if let Some(p) = world.get::<ResourcePatch>(e) {
            let mut cmd = VillageAct::new(ctx.unit, "gather");
            cmd.target = Some(id);
            out.push(checked(
                Affordance::new(
                    "gather",
                    format!(
                        "Gather {} ({} left, {} ticks)",
                        p.kind, p.remaining, p.gather_ticks
                    ),
                )
                .target(id),
                cmd.check(world),
            ));
        }
        if world.get::<DroppedDiary>(e).is_some()
            && world
                .get::<CarriedBy>(e)
                .is_some_and(|c| c.holder == ctx.unit)
        {
            let mut c = VillageAct::new(ctx.unit, "read_diary");
            c.target = Some(id);
            out.push(checked(
                Affordance::new("read_diary", "Read a page of the recovered diary you carry")
                    .target(id)
                    .arg("page", "page number starting at 1; default 1"),
                c.check(world),
            ));
        }
        if world.get::<CoinPurse>(e).is_some() {
            let mut c = VillageAct::new(ctx.unit, "collect_coins");
            c.target = Some(id);
            out.push(checked(
                Affordance::new("collect_coins", "Collect the dropped coins").target(id),
                c.check(world),
            ));
        }
    }
    out
}
pub fn handle(world: &World, ctx: &AgentCtx, req: &ActionRequest) -> Result<ActionPlan, Rejection> {
    for key in req.args.as_object().into_iter().flat_map(|m| m.keys()) {
        if !["kind", "quantity", "page", "title", "body"].contains(&key.as_str()) {
            return Err(Rejection::new(
                "invalid_args",
                format!("unknown village argument `{key}`"),
            ));
        }
    }
    let mut cmd = VillageAct::new(ctx.unit, &req.verb);
    cmd.target = req.target;
    cmd.kind = req.opt_str("kind")?.unwrap_or("").into();
    cmd.quantity = match req.opt_int("quantity")? {
        None => 1,
        Some(n) => u32::try_from(n)
            .map_err(|_| Rejection::new("invalid_args", "quantity must be 1..=20"))?,
    };
    cmd.title = req.opt_str("title")?.unwrap_or("").into();
    cmd.page = match req.opt_int("page")? {
        None => 1,
        Some(n) => u32::try_from(n)
            .map_err(|_| Rejection::new("invalid_args", "page must be a positive number"))?,
    };
    cmd.body = req.opt_str("body")?.unwrap_or("").into();
    cmd.check(world)
        .map_err(|why| Rejection::new("unavailable", why))?;
    Ok(ActionPlan::Commands {
        label: format!("Village {}", cmd.verb),
        commands: vec![SimCommand::content(&cmd)],
    })
}
pub fn describe(world: &World, id: SimId) -> Option<String> {
    let e = entity_of(world, id)?;
    if let Some(v) = world.get::<Villager>(e) {
        return Some(format!(
            "{}, {} #{}{}",
            v.name,
            v.role.name(),
            id.0,
            if living(world, e) { "" } else { " (dead)" }
        ));
    }
    if let Some(p) = world.get::<VillagePlace>(e) {
        return Some(format!("{} #{}", p.label, id.0));
    }
    if let Some(p) = world.get::<ResourcePatch>(e) {
        return Some(format!("{} patch #{}", p.kind, id.0));
    }
    if let Some(d) = world.get::<DroppedDiary>(e) {
        return Some(format!(
            "{}'s diary (life {}) #{}",
            d.author_name, d.life, id.0
        ));
    }
    let item = world.get::<Item>(e)?;
    match item.kind.as_str() {
        catalog::GRAIN => Some(format!("grain #{}", id.0)),
        catalog::FLOUR => Some(format!("flour #{}", id.0)),
        catalog::WATER => Some(format!("clean water #{}", id.0)),
        catalog::BOWL => Some(format!("wooden bowl #{}", id.0)),
        _ => None,
    }
}
