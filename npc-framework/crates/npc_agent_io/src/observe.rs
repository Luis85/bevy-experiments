//! Egocentric observations (spec §2.3) and the deterministic text renderer.
//!
//! JSON shape:
//!
//! ```text
//! {"unit":5,"kind":"villager","tick":1234,"seq":17,"epoch":3,"day":0,"time_permille":514,"night":false,
//!  "omniscient":true,
//!  "controller":{"id":"agent","mode":"Direct","on_idle":"Idle"} | null,
//!  "queue":{"current":{"handle":3,"verb":"walk_to","label":"..","step":1,"steps":2}|null,
//!           "pending":[{"handle":4,"verb":"eat","label":".."}]},
//!  "sections":{"self":{..},"needs":{..},..},       // by provider name
//!  "recent_events":[AgentEvent],                    // significant, oldest first
//!  "affordances":[Affordance],
//!  "hidden_affordances":3,                          // only with `brief`
//!  "changes":{..}}                                  // only with `since_seq`: [`changes`]
//! ```
//!
//! With `since_seq` the text form starts with one [`since_line`] ("Since
//! your last look: ..." or "Nothing changed since your last look.").
//!
//! `omniscient` is true until a perception provider is registered and the
//! unit has a perception model (senses): the world sections then show
//! everything, not only what the unit perceives. With one, **affordances
//! only target entities the unit knows** (perceives,
//! remembers, holds or rides; see [`known_targets`]): whatever a provider
//! lists about anything else is dropped here, centrally. The text form is a
//! compact, prompt-ready narrative built only from this JSON plus the
//! providers' lines, so the same state always renders the same text.

use std::collections::BTreeSet;

use bevy_ecs::prelude::*;
use npc_kernel::inspect;
use npc_kernel::{SimId, SimIndex};
use serde_json::{Value, json};

use crate::control::{ExternalController, ExternalQueue};
use crate::events::{self, AgentEvent};
use crate::registry::{Affordance, AgentCtx, AgentRegistry};

/// Recent events in an observation by default.
pub const DEFAULT_RECENT_EVENTS: usize = 10;

/// What `ctx.unit` holds or rides (the possessions providers), ascending.
pub fn possessions(world: &World, ctx: &AgentCtx) -> BTreeSet<SimId> {
    world
        .get_resource::<AgentRegistry>()
        .map(|r| r.possessions.values().flat_map(|f| f(world, ctx)).collect())
        .unwrap_or_default()
}

/// The entities `ctx.unit` knows when it has a perception model (a
/// perception provider is registered and a known provider answers for this
/// unit): itself, what it perceives or remembers and its possessions.
/// `None` otherwise: no gate, everything is known (`omniscient`).
pub fn known_targets(world: &World, ctx: &AgentCtx) -> Option<BTreeSet<SimId>> {
    let registry = world.get_resource::<AgentRegistry>()?;
    if !registry.has_perception() {
        return None;
    }
    let mut modelled = false;
    let mut known = BTreeSet::new();
    for f in registry.known.values() {
        if let Some(ids) = f(world, ctx) {
            modelled = true;
            known.extend(ids);
        }
    }
    if !modelled {
        return None;
    }
    known.extend(registry.possessions.values().flat_map(|f| f(world, ctx)));
    known.insert(ctx.unit);
    Some(known)
}

/// The affordances of `unit` now, in provider-name order. With perception,
/// those whose target the unit does not know are dropped (see the module
/// docs). A verb a verb gate refuses ([`crate::verb_gate`]) is offered
/// unavailable and structural (`brief` hides it), the gate's reason as
/// `why_not`.
pub fn affordances(world: &World, ctx: &AgentCtx) -> Vec<Affordance> {
    let Some(registry) = world.get_resource::<AgentRegistry>() else {
        return Vec::new();
    };
    let known = known_targets(world, ctx);
    registry
        .affordances
        .values()
        .flat_map(|f| f(world, ctx))
        .filter(|a| match (&known, a.target) {
            (Some(k), Some(t)) => k.contains(&t),
            _ => true,
        })
        .map(|a| match crate::verb_gate(world, ctx.entity, &a.verb) {
            Some(reason) => a.structural(reason),
            None => a,
        })
        .collect()
}

/// A short description of `id`: the first describer that knows it
/// (`"store_key #26"`, `"the locked door #12"`), else `"<kind> #id"`.
pub fn describe(world: &World, id: SimId) -> String {
    if let Some(d) = world
        .get_resource::<AgentRegistry>()
        .and_then(|r| r.describers.values().find_map(|f| f(world, id)))
    {
        return d;
    }
    match world.get_resource::<SimIndex>().and_then(|i| i.get(id)) {
        Some(e) => format!("{} #{}", inspect::kind_name(world, e), id.0),
        None => format!("#{}", id.0),
    }
}

/// The context of `unit` (`None` for an unknown unit).
pub fn ctx(world: &World, unit: SimId) -> Option<AgentCtx<'_>> {
    let entity = world.get_resource::<SimIndex>()?.get(unit)?;
    Some(AgentCtx {
        unit,
        entity,
        controller: world.get::<ExternalController>(entity),
        planned: &[],
    })
}

fn queue_json(world: &World, entity: Entity) -> Value {
    let Some(q) = world.get::<ExternalQueue>(entity) else {
        return Value::Null;
    };
    let current = q.current.as_ref().map(|c| {
        json!({
            "handle": c.item.handle,
            "verb": c.item.verb,
            "label": c.item.label,
            "step": c.step,
            "steps": c.item.steps.len(),
        })
    });
    let pending: Vec<Value> = q
        .pending
        .iter()
        .map(|i| json!({"handle": i.handle, "verb": i.verb, "label": i.label}))
        .collect();
    json!({"current": current, "pending": pending})
}

/// Observation options.
#[derive(Clone, Copy, Debug)]
pub struct ObserveOptions {
    /// Recent events to include.
    pub recent: usize,
    /// Hide structurally impossible affordances
    /// ([`Affordance::structurally_impossible`]).
    pub brief: bool,
}

impl Default for ObserveOptions {
    fn default() -> Self {
        Self {
            recent: DEFAULT_RECENT_EVENTS,
            brief: false,
        }
    }
}

/// `unit`'s observation as JSON plus the providers' text lines (in
/// provider-name order); `None` for an unknown unit.
pub fn observe(world: &World, unit: SimId, recent: usize) -> Option<(Value, Vec<String>)> {
    observe_with(
        world,
        unit,
        ObserveOptions {
            recent,
            brief: false,
        },
    )
}

/// [`observe`] with options.
pub fn observe_with(
    world: &World,
    unit: SimId,
    opts: ObserveOptions,
) -> Option<(Value, Vec<String>)> {
    let ctx = ctx(world, unit)?;
    let clock = inspect::clock(world);
    let registry = world.get_resource::<AgentRegistry>();
    let mut sections = serde_json::Map::new();
    let mut lines = Vec::new();
    for (name, (f, _)) in registry.iter().flat_map(|r| r.observations.iter()) {
        if let Some(section) = f(world, &ctx) {
            sections.insert((*name).to_owned(), section.value);
            lines.extend(section.lines);
        }
    }
    let controller = ctx
        .controller
        .map(|c| json!({"id": c.id, "mode": c.mode.name(), "on_idle": c.on_idle.name()}));
    let recent: Vec<AgentEvent> = events::recent(world, unit, opts.recent);
    let mut affs = affordances(world, &ctx);
    let before = affs.len();
    if opts.brief {
        affs.retain(|a| !a.structurally_impossible());
    }
    let mut obs = json!({
        "unit": unit,
        "kind": inspect::kind_name(world, ctx.entity),
        "tick": clock.tick,
        "seq": events::last_seq(world),
        "epoch": crate::api::epoch(world),
        "day": clock.day,
        "time_permille": clock.time_permille,
        "night": clock.night,
        "omniscient": known_targets(world, &ctx).is_none(),
        "controller": controller,
        "queue": queue_json(world, ctx.entity),
        "sections": sections,
        "recent_events": recent,
        "affordances": affs,
    });
    if opts.brief {
        obs["hidden_affordances"] =
            json!(before - obs["affordances"].as_array().map_or(0, Vec::len));
    }
    Some((obs, lines))
}

/// Entries per kind of change named in the "Since your last look" line
/// (the JSON lists them all).
pub const SINCE_TEXT_LIMIT: usize = 8;

/// What changed for `unit` since event `since_seq` (`agent/observe
/// {since_seq}`), built only from the agent event buffer (tooling, never sim
/// state):
///
/// ```text
/// {"since_seq":S, "truncated":false,
///  "sensed_new":[{"id","name","via"}], "lost":[{"id","name","forgotten"}],
///  "inventory_gained":[{"item","kind","stack","name","from"}],
///  "inventory_lost":[{"item","kind","stack","name","to"}],
///  "orders":[{"order","result":"done"|"failed","reason"?,"nearest_reachable"?,"blocker"?}],
///  "interactions":[{"verb","target","name","outcome","item"?,"reason"?,"dropped"?}],
///  "actions":[{"handle","verb","status"}]}
/// ```
///
/// Percepts and items are net: a target's last `Perceived`/`Lost` decides
/// which list it is in (ascending id; what the unit holds or rides now is
/// left out, like in the perception section), an item counts as gained when the
/// unit did not hold it before its first move in the window and holds it
/// after its last (lost: the other way round), whoever moved it. Orders and
/// interactions (ended: not `started`) are listed oldest first; the unit's
/// own actions (`ExternalAction`) once per handle, in order of first
/// appearance, with their latest status (`queued`, `started`, `done`,
/// `failed`, `cancelled`, `rejected`).
/// `truncated`: the ring buffer already evicted events after `since_seq`.
pub fn changes(world: &World, unit: SimId, since_seq: u64) -> Value {
    use npc_kernel::journal::JournalKind as K;
    use std::collections::BTreeMap;

    let Some(ev) = world.get_resource::<events::AgentEvents>() else {
        return json!({
            "since_seq": since_seq, "truncated": false, "sensed_new": [], "lost": [],
            "inventory_gained": [], "inventory_lost": [], "orders": [], "interactions": [],
            "actions": [],
        });
    };
    let truncated = ev.dropped > 0 && ev.oldest_seq() > since_seq.saturating_add(1);
    let mine = |e: &AgentEvent| e.unit == Some(unit);
    // target -> (sensed now, entry)
    let mut percepts: BTreeMap<SimId, (bool, Value)> = BTreeMap::new();
    // item -> (held before, held after, kind, stack, from, to)
    type ItemMove = (bool, bool, String, u32, Option<SimId>, Option<SimId>);
    let mut items: BTreeMap<SimId, ItemMove> = BTreeMap::new();
    let (mut orders, mut interactions, mut actions) = (Vec::new(), Vec::new(), Vec::new());
    // handle -> index in `actions`
    let mut action_at: BTreeMap<u32, usize> = BTreeMap::new();
    for e in ev.after(since_seq) {
        match &e.event {
            K::Perceived { target, via } if mine(e) => {
                let entry = json!({"id": target, "name": describe(world, *target), "via": via});
                percepts.insert(*target, (true, entry));
            }
            K::Lost { target, forgotten } if mine(e) => {
                let entry =
                    json!({"id": target, "name": describe(world, *target), "forgotten": forgotten});
                percepts.insert(*target, (false, entry));
            }
            K::ItemMoved {
                item,
                kind,
                stack,
                from,
                to,
                ..
            } if *from == Some(unit) || *to == Some(unit) => {
                let m = items.entry(*item).or_insert((
                    *from == Some(unit),
                    false,
                    String::new(),
                    0,
                    *from,
                    None,
                ));
                m.1 = *to == Some(unit);
                m.2.clone_from(kind);
                m.3 = *stack;
                m.5 = *to;
            }
            K::OrderDone { order } if mine(e) => {
                orders.push(json!({"order": order, "result": "done"}));
            }
            K::OrderFailed {
                order,
                reason,
                nearest_reachable,
                blocker,
            } if mine(e) => {
                let mut o = json!({"order": order, "result": "failed", "reason": reason});
                if let Some(p) = nearest_reachable {
                    o["nearest_reachable"] = json!(p);
                }
                if let Some(b) = blocker {
                    o["blocker"] = json!(b);
                }
                orders.push(o);
            }
            K::Interacted {
                verb,
                target,
                item,
                outcome,
                reason,
                dropped,
            } if mine(e) && outcome != "started" => {
                let mut i = json!({"verb": verb, "target": target,
                                   "name": describe(world, *target), "outcome": outcome});
                if let Some(it) = item {
                    i["item"] = json!(it);
                }
                if let Some(r) = reason {
                    i["reason"] = json!(r);
                }
                if !dropped.is_empty() {
                    i["dropped"] = json!(dropped);
                }
                interactions.push(i);
            }
            K::ExternalAction {
                handle,
                verb,
                status,
                ..
            } if mine(e) => match action_at.get(handle) {
                Some(&i) => actions[i] = json!({"handle": handle, "verb": verb, "status": status}),
                None => {
                    action_at.insert(*handle, actions.len());
                    actions.push(json!({"handle": handle, "verb": verb, "status": status}));
                }
            },
            _ => {}
        }
    }
    // What the unit holds or rides is not news as a percept (the perception
    // section leaves possessions out too): picking a thing up loses sight of it.
    let held = ctx(world, unit)
        .map(|c| possessions(world, &c))
        .unwrap_or_default();
    let (mut sensed_new, mut lost) = (Vec::new(), Vec::new());
    for (target, (sensed, entry)) in percepts {
        if held.contains(&target) {
            continue;
        }
        if sensed {
            sensed_new.push(entry);
        } else {
            lost.push(entry);
        }
    }
    let (mut gained, mut given) = (Vec::new(), Vec::new());
    for (item, (before, after, kind, stack, from, to)) in items {
        let name = describe(world, item);
        match (before, after) {
            (false, true) => gained.push(
                json!({"item": item, "kind": kind, "stack": stack, "name": name, "from": from}),
            ),
            (true, false) => given
                .push(json!({"item": item, "kind": kind, "stack": stack, "name": name, "to": to})),
            _ => {}
        }
    }
    json!({
        "since_seq": since_seq, "truncated": truncated,
        "sensed_new": sensed_new, "lost": lost,
        "inventory_gained": gained, "inventory_lost": given,
        "orders": orders, "interactions": interactions, "actions": actions,
    })
}

/// `items` rendered by `f`, at most [`SINCE_TEXT_LIMIT`], joined by ", ".
fn listed(items: &Value, f: impl Fn(&Value) -> String) -> Option<String> {
    let items = items.as_array().filter(|a| !a.is_empty())?;
    let mut out: Vec<String> = items.iter().take(SINCE_TEXT_LIMIT).map(f).collect();
    if items.len() > SINCE_TEXT_LIMIT {
        out.push(format!("and {} more", items.len() - SINCE_TEXT_LIMIT));
    }
    Some(out.join(", "))
}

/// The first text line of an observation with `since_seq`, from its
/// [`changes`]: `"Since your last look: action #3 walk_to done; order Move
/// done; gained book #26; you now sense door #12."`, or `"Nothing changed
/// since your last look."`.
pub fn since_line(changes: &Value) -> String {
    let name = |v: &Value| v["name"].as_str().unwrap_or("?").to_owned();
    let mut parts: Vec<String> = Vec::new();
    let mut push = |p: Option<String>| parts.extend(p);
    push(listed(&changes["actions"], |a| {
        format!(
            "action #{} {} {}",
            a["handle"],
            s(a, "verb"),
            s(a, "status")
        )
    }));
    push(listed(&changes["orders"], |o| match o["reason"].as_str() {
        Some(_) => format!(
            "order {} {}: {}",
            s(o, "order"),
            s(o, "result"),
            order_failed_text(o)
        ),
        None => format!("order {} {}", s(o, "order"), s(o, "result")),
    }));
    push(listed(&changes["interactions"], |i| {
        let mut d = format!("{} {}: {}", s(i, "verb"), name(i), s(i, "outcome"));
        if let Some(dt) = dropped_text(i) {
            d.push_str(&format!(", {dt}"));
        }
        if let Some(r) = i["reason"].as_str() {
            d.push_str(&format!(" ({r})"));
        }
        d
    }));
    let stacked = |v: &Value| match v["stack"].as_u64() {
        Some(n) if n > 1 => format!("{} (x{n})", name(v)),
        _ => name(v),
    };
    push(listed(&changes["inventory_gained"], stacked).map(|l| format!("gained {l}")));
    push(listed(&changes["inventory_lost"], stacked).map(|l| format!("no longer carry {l}")));
    push(listed(&changes["sensed_new"], name).map(|l| format!("you now sense {l}")));
    let lost = changes["lost"].as_array().cloned().unwrap_or_default();
    let (forgot, unseen): (Vec<Value>, Vec<Value>) = lost
        .into_iter()
        .partition(|l| l["forgotten"].as_bool() == Some(true));
    push(listed(&json!(unseen), name).map(|l| format!("you no longer sense {l}")));
    push(listed(&json!(forgot), name).map(|l| format!("you forgot {l}")));
    let truncated = changes["truncated"].as_bool() == Some(true);
    match (parts.is_empty(), truncated) {
        (true, false) => "Nothing changed since your last look.".to_owned(),
        (true, true) => {
            "Since your last look (older events were dropped): nothing recorded.".to_owned()
        }
        (false, false) => format!("Since your last look: {}.", parts.join("; ")),
        (false, true) => format!(
            "Since your last look (older events were dropped, so this is incomplete): {}.",
            parts.join("; ")
        ),
    }
}

/// The text form of an `agent/act_and_wait` result (the MCP tool, `npcctl
/// agent act --wait --format text`): a line on the act, a line on the wait,
/// then the observation's text (its JSON when it was observed as JSON).
pub fn act_and_wait_text(r: &Value) -> String {
    let act = &r["act"];
    let mut out = Vec::new();
    let accepted = act["accepted"].as_bool() == Some(true);
    if accepted {
        let handle = if act["handle"].is_null() {
            String::new()
        } else {
            format!("#{} ", act["handle"])
        };
        out.push(format!(
            "Act accepted: {handle}{} ({}).",
            s(act, "verb"),
            s(act, "label")
        ));
        let ev = &r["events"];
        let n = ev["events"].as_array().map_or(0, Vec::len);
        out.push(if ev["timed_out"].as_bool() == Some(true) {
            match ev["busy"].as_str() {
                Some(b) => format!("The wait timed out after {n} event(s); still busy with {b}."),
                None => format!("The wait timed out after {n} event(s)."),
            }
        } else if ev["idle"].as_bool() == Some(true) {
            format!("Done: idle again after {n} event(s).")
        } else {
            format!("The wait ended at a matching event ({n} event(s)).")
        });
    } else {
        out.push(format!(
            "Act rejected ({}): {}",
            s(act, "reason"),
            s(act, "message")
        ));
    }
    let obs = &r["observation"];
    match obs.get("text").and_then(Value::as_str) {
        Some(t) => out.push(t.trim_end().to_owned()),
        None if obs.is_null() => out.push(format!(
            "No observation: {}",
            r["observation_error"]["message"].as_str().unwrap_or("?")
        )),
        None => out.push(obs.to_string()),
    }
    let mut text = out.join("\n");
    text.push('\n');
    text
}

/// `mm` as metres with two decimals (`-1234` → `"-1.23"`), integer math.
pub fn metres(mm: i64) -> String {
    let sign = if mm < 0 { "-" } else { "" };
    let a = mm.unsigned_abs();
    format!("{sign}{}.{:02}", a / 1000, (a % 1000) / 10)
}

/// `(x, z)` mm as `"(1.23 m, -0.05 m)"`, integer math.
pub fn metres_pos(x: i64, z: i64) -> String {
    format!("({} m, {} m)", metres(x), metres(z))
}

/// The `dropped` list of an `Interacted` (its event or `changes` JSON) as
/// text, grouped by position: `"2 × log dropped at (24.08 m, 1.10 m)"`,
/// `"2 × log, 1 × ore dropped at (..)"`; `None` when nothing was dropped.
pub fn dropped_text(ev: &Value) -> Option<String> {
    let list = ev["dropped"].as_array().filter(|l| !l.is_empty())?;
    let mut groups: Vec<(String, Vec<String>)> = Vec::new();
    for d in list {
        let n = |i: usize| d["pos"].get(i).and_then(Value::as_i64).unwrap_or(0);
        let at = metres_pos(n(0), n(1));
        let what = format!("{} × {}", d["stack"].as_u64().unwrap_or(1), s(d, "kind"));
        match groups.last_mut() {
            Some((p, items)) if *p == at => items.push(what),
            _ => groups.push((at, vec![what])),
        }
    }
    let parts: Vec<String> = groups
        .into_iter()
        .map(|(at, items)| format!("{} dropped at {at}", items.join(", ")))
        .collect();
    Some(parts.join("; "))
}

/// The detail of an `Interacted` after its outcome: `": <reason>"`, or for
/// a success that dropped items `", 2 × log dropped at (24.08 m, 1.10 m):
/// pack would carry 30950 of 30000 g"`.
fn interacted_tail(ev: &Value) -> String {
    let mut out = String::new();
    if let Some(d) = dropped_text(ev) {
        out.push_str(&format!(", {d}"));
    }
    if let Some(r) = ev["reason"].as_str() {
        out.push_str(&format!(": {r}"));
    }
    out
}

/// The reason of an `OrderFailed` (its event or `changes` JSON) with the
/// blocked-walk report when it has one: `"blocked by locked door #12;
/// nearest reachable (11.20 m, 3.40 m)"`, `"blocked by a wall at (5.50 m,
/// 8.50 m); .."`, `"blocked by the edge of the world; .."`.
pub fn order_failed_text(ev: &Value) -> String {
    let mut out = s(ev, "reason").to_owned();
    let b = &ev["blocker"];
    let at = |v: &Value| {
        let n = |i: usize| v.get(i).and_then(Value::as_i64).unwrap_or(0);
        metres_pos(n(0), n(1))
    };
    match b["kind"].as_str() {
        Some("Door") => out.push_str(&format!(
            " by {} door #{}",
            if b["locked"].as_bool() == Some(true) {
                "locked"
            } else {
                "closed"
            },
            b["id"]
        )),
        Some("Wall") => out.push_str(&format!(" by a wall at {}", at(&b["pos"]))),
        Some("OutOfBounds") => out.push_str(" by the edge of the world"),
        _ => {}
    }
    if ev["nearest_reachable"].is_array() {
        out.push_str(&format!(
            "; nearest reachable {}",
            at(&ev["nearest_reachable"])
        ));
    }
    out
}

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v[k].as_str().unwrap_or("")
}

/// One recent-event line (also used by `agent/act` stale messages).
pub fn event_line(e: &Value) -> String {
    let tick = e["tick"].as_u64().unwrap_or(0);
    let ev = &e["event"];
    let kind = ev["type"].as_str().unwrap_or("?");
    let detail = match kind {
        "ExternalAction" => format!(
            "action #{} {} {}",
            ev["handle"],
            s(ev, "verb"),
            s(ev, "status")
        ),
        "NeedLevel" => format!("{} {} -> {}", s(ev, "need"), s(ev, "from"), s(ev, "to")),
        "ActionRejected" => format!("{} rejected: {}", s(ev, "verb"), s(ev, "reason")),
        "Roll" => {
            let target = ev
                .get("target_number")
                .or_else(|| ev.get("target"))
                .cloned()
                .unwrap_or(Value::Null);
            format!(
                "{} {} vs {target}: rolled {}, {} (margin {}){}",
                s(ev, "purpose"),
                s(ev, "skill"),
                ev["roll"],
                if ev["success"].as_bool() == Some(true) {
                    "success"
                } else {
                    "failure"
                },
                ev["margin"],
                match s(ev, "crit") {
                    "" | "None" => String::new(),
                    c => format!(", critical {c}"),
                }
            )
        }
        "Interacted" => format!(
            "{} #{} {}{}",
            s(ev, "verb"),
            ev["target"],
            s(ev, "outcome"),
            interacted_tail(ev)
        ),
        "OrderDone" => format!("{} done", s(ev, "order")),
        "OrderFailed" => format!("{} failed: {}", s(ev, "order"), order_failed_text(ev)),
        _ => {
            let mut obj = ev.clone();
            if let Value::Object(m) = &mut obj {
                m.remove("type");
            }
            if obj.as_object().is_some_and(serde_json::Map::is_empty) {
                String::new()
            } else {
                obj.to_string()
            }
        }
    };
    format!("- t{tick} {kind} {detail}").trim_end().to_owned()
}

/// The text form of an observation (see the module docs).
pub fn render_text(obs: &Value, lines: &[String]) -> String {
    let mut out = Vec::new();
    let kind = obs["kind"].as_str().unwrap_or("unit");
    let mut head = format!("You are {kind} #{}", obs["unit"]);
    if let Some(c) = obs["controller"].as_object() {
        head.push_str(&format!(
            " (controller `{}`, {} mode, on idle: {})",
            c["id"].as_str().unwrap_or(""),
            c["mode"].as_str().unwrap_or(""),
            c["on_idle"].as_str().unwrap_or("")
        ));
    }
    out.push(format!("{head}."));
    let permille = obs["time_permille"].as_u64().unwrap_or(0);
    out.push(format!(
        "Tick {} (day {}, {}.{}% of the day, {}), seq {}.",
        obs["tick"],
        obs["day"],
        permille / 10,
        permille % 10,
        if obs["night"].as_bool() == Some(true) {
            "night"
        } else {
            "day time"
        },
        obs["seq"]
    ));
    if obs["omniscient"].as_bool() == Some(true) {
        out.push("(No perception model: you are shown the whole world.)".to_owned());
    }
    out.extend(lines.iter().cloned());
    if let Some(q) = obs["queue"].as_object() {
        match q.get("current").and_then(Value::as_object) {
            Some(c) => out.push(format!(
                "Doing: #{} {} (step {}/{}).",
                c["handle"],
                c["label"].as_str().unwrap_or(""),
                c["step"].as_u64().unwrap_or(0) + 1,
                c["steps"]
            )),
            None => out.push("Doing: nothing (waiting for your next action).".to_owned()),
        }
        let pending: Vec<String> = q
            .get("pending")
            .and_then(Value::as_array)
            .map(|p| {
                p.iter()
                    .map(|i| format!("#{} {}", i["handle"], i["label"].as_str().unwrap_or("")))
                    .collect()
            })
            .unwrap_or_default();
        if !pending.is_empty() {
            out.push(format!("Queued: {}.", pending.join("; ")));
        }
    }
    if let Some(events) = obs["recent_events"].as_array()
        && !events.is_empty()
    {
        out.push("Recent events:".to_owned());
        out.extend(events.iter().map(event_line));
    }
    if let Some(affs) = obs["affordances"].as_array() {
        out.push("You can:".to_owned());
        for a in affs {
            let mut line = format!(
                "- [{}] {}",
                a["id"].as_str().unwrap_or(""),
                a["label"].as_str().unwrap_or("")
            );
            if let Some(t) = a["est_ticks"].as_u64() {
                line.push_str(&format!(" (~{t} ticks)"));
            }
            if let Some(args) = a["args"].as_object()
                && !args.is_empty()
            {
                let names: Vec<String> = args
                    .iter()
                    .map(|(k, v)| format!("{k}: {}", v.as_str().unwrap_or("")))
                    .collect();
                line.push_str(&format!(" args {{{}}}", names.join(", ")));
            }
            if a["ok"].as_bool() == Some(false) {
                line.push_str(&format!(
                    " -- unavailable: {}",
                    a["why_not"].as_str().unwrap_or("")
                ));
            }
            out.push(line);
        }
        if let Some(n) = obs["hidden_affordances"].as_u64()
            && n > 0
        {
            out.push(format!(
                "({n} structurally impossible action(s) hidden: brief view.)"
            ));
        }
    }
    let mut text = out.join("\n");
    text.push('\n');
    text
}

/// Built-in section `self`: kind, position (mm), running tree path and leaf.
pub fn self_section(world: &World, ctx: &AgentCtx) -> Option<crate::registry::Section> {
    let pos = world.get::<npc_kernel::SimPos>(ctx.entity).copied();
    let leaf = inspect::active_leaf(world, ctx.entity);
    let value = json!({
        "id": ctx.unit,
        "kind": inspect::kind_name(world, ctx.entity),
        "pos": pos.map(|p| [p.x, p.z]),
        "tree_path": inspect::tree_path(world, ctx.entity),
        "active_action": leaf.as_ref().map(|l| l.action.clone()),
    });
    let mut section = crate::registry::Section::new(value);
    if let Some(p) = pos {
        section = section.line(format!(
            "Position: ({} m, {} m).",
            metres(i64::from(p.x)),
            metres(i64::from(p.z))
        ));
    }
    Some(section)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interacted_names_its_drops() {
        let e = json!({"tick": 9, "event": {"type": "Interacted", "verb": "use_tool_on",
            "target": 42, "outcome": "success", "reason": "pack would carry 30950 of 30000 g",
            "dropped": [{"item": 70, "kind": "log", "stack": 2, "pos": [24080, 1100]}]}});
        assert_eq!(
            event_line(&e),
            "- t9 Interacted use_tool_on #42 success, 2 × log dropped at (24.08 m, 1.10 m): \
             pack would carry 30950 of 30000 g"
        );
        let plain = json!({"tick": 9, "event": {"type": "Interacted", "verb": "open",
            "target": 12, "outcome": "failed", "reason": "door is locked"}});
        assert_eq!(
            event_line(&plain),
            "- t9 Interacted open #12 failed: door is locked"
        );
    }

    #[test]
    fn metres_are_integer_formatted() {
        assert_eq!(metres(0), "0.00");
        assert_eq!(metres(1234), "1.23");
        assert_eq!(metres(-50), "-0.05");
        assert_eq!(metres(-12_345), "-12.34");
    }

    #[test]
    fn roll_lines_name_the_target_number() {
        let e = json!({"tick": 9, "event": {"type": "Roll", "skill": "Mining", "target_number": 11,
            "roll": 9, "margin": 2, "success": true, "crit": "None", "purpose": "interaction.use_tool_on"}});
        assert_eq!(
            event_line(&e),
            "- t9 Roll interaction.use_tool_on Mining vs 11: rolled 9, success (margin 2)"
        );
    }
}
