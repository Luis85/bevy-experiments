//! `inspect` (the world or one entity at a tick) and `bisect-desync` (where two runs part).

use std::collections::BTreeSet;
use std::path::Path;

use bevy_ecs::world::World;
use fl_protocol::SimId;
use fl_sim::{SimIndex, compute_state_hash, inspect_entity, inspect_world};
use serde_json::{Value, json};

use crate::agent::{Game, ScenarioOptions, Session, SessionConfig, Source};
use crate::report::{CliError, Report, hash_hex};

/// `inspect`: re-simulate `source` to `tick` and print [`inspect_world`], or
/// [`inspect_entity`] of `entity`.
///
/// # Errors
/// [`CliError::Check`] if no entity has the id at that tick, [`CliError::Usage`] if the tick
/// is out of range;
/// any error setting the source up.
pub fn inspect(
    game: &Game,
    source: &Source,
    options: &ScenarioOptions,
    tick: u32,
    entity: Option<u32>,
) -> Result<Report, CliError> {
    let mut session = Session::open(game, source, options, SessionConfig::default())?;
    session.run_to(tick)?;
    let world = session.world();
    let json = match entity {
        None => inspect_world(world),
        Some(id) => inspect_entity(world, SimId(id))
            .ok_or_else(|| CliError::Check(format!("no entity {id} at tick {tick}")))?,
    };
    let text = format!("{json:#}\n");
    Ok(Report::ok(json, text))
}

/// The ids, in order, whose [`inspect_entity`] differs between `a` and `b` (including ids
/// only one of them has).
fn differing_entities(a: &World, b: &World) -> Vec<u32> {
    let ids: BTreeSet<SimId> = a
        .resource::<SimIndex>()
        .ids()
        .chain(b.resource::<SimIndex>().ids())
        .collect();
    ids.into_iter()
        .filter(|&id| inspect_entity(a, id) != inspect_entity(b, id))
        .map(|id| id.0)
        .collect()
}

/// `bisect-desync`: re-simulate the replays (or run directories) `a` and `b` side by side
/// with a state hash every tick and report the first tick whose hashes differ. Hashes are
/// taken after each tick runs, so that tick is the one whose inputs (or rules) differ; the
/// report names the entities whose [`inspect_entity`] differs right after it.
///
/// JSON: `{"diverged", "tick", "setup", "hash_a", "hash_b", "entities", "ticks_compared",
/// "ticks_a", "ticks_b"}`. `setup` is true (and `tick` null) when the two differ before their
/// first tick. Exit 1 when they diverge; exit 1 too, with a `message`, when they match over
/// their common ticks but have different lengths; exit 0 only when both have the same ticks
/// and every one matches.
///
/// # Errors
/// [`CliError::Usage`] if a source is not a replay or the two start at different ticks;
/// [`CliError::Check`] if one of them has no ticks (nothing to compare); any error setting
/// them up.
pub fn bisect_desync(game: &Game, a: &Path, b: &Path) -> Result<Report, CliError> {
    let open = |path: &Path| -> Result<Session, CliError> {
        let source = Source::parse(&path.to_string_lossy());
        if !matches!(source, Source::Replay(_)) {
            return Err(CliError::Usage(format!(
                "bisect-desync compares replays or run directories, not {}",
                path.display()
            )));
        }
        let config = SessionConfig {
            hash_interval: Some(1),
            journal: false,
        };
        Session::open(game, &source, &ScenarioOptions::default(), config)
    };
    let (mut sa, mut sb) = (open(a)?, open(b)?);
    if sa.next_tick() != sb.next_tick() {
        return Err(CliError::Usage(format!(
            "the runs start at different ticks ({} and {})",
            sa.next_tick().0,
            sb.next_tick().0
        )));
    }
    let start = sa.next_tick().0;
    let ticks = |s: &Session| s.end().map_or(0, |end| end.0 - start);
    let (ticks_a, ticks_b) = (ticks(&sa), ticks(&sb));
    let compared = ticks_a.min(ticks_b);
    if compared == 0 {
        return Err(CliError::Check(format!(
            "nothing to compare: {} has {ticks_a} ticks, {} has {ticks_b}",
            a.display(),
            b.display()
        )));
    }

    let report = |tick: Option<u32>, ha: u64, hb: u64, wa: &World, wb: &World, n: u32| {
        let entities = differing_entities(wa, wb);
        let at = tick.map_or_else(
            || "before the first tick".to_owned(),
            |t| format!("tick {t}"),
        );
        let json = json!({
            "diverged": true, "tick": tick, "setup": tick.is_none(),
            "hash_a": hash_hex(ha), "hash_b": hash_hex(hb),
            "entities": entities, "ticks_compared": n,
            "ticks_a": ticks_a, "ticks_b": ticks_b,
        });
        let text = format!(
            "diverged at {at}: {} vs {}\nentities that differ: {entities:?}\n",
            hash_hex(ha),
            hash_hex(hb)
        );
        Report {
            json,
            text,
            code: 1,
        }
    };

    let (ha, hb) = (
        compute_state_hash(sa.world()),
        compute_state_hash(sb.world()),
    );
    if ha != hb {
        return Ok(report(None, ha, hb, sa.world(), sb.world(), 0));
    }
    for n in 1..=compared {
        let (oa, ob) = (sa.step()?, sb.step()?);
        if oa.hash != ob.hash {
            let (ha, hb) = (oa.hash.unwrap_or(0), ob.hash.unwrap_or(0));
            return Ok(report(Some(oa.tick.0), ha, hb, sa.world(), sb.world(), n));
        }
    }
    let mut json = json!({
        "diverged": false, "tick": Value::Null, "setup": false,
        "entities": [], "ticks_compared": compared,
        "ticks_a": ticks_a, "ticks_b": ticks_b,
    });
    if ticks_a == ticks_b {
        let text = format!("identical over {compared} ticks\n");
        return Ok(Report::ok(json, text));
    }
    let message = format!(
        "identical over the {compared} common ticks, but the runs have different lengths \
         ({ticks_a} and {ticks_b} ticks)"
    );
    json["message"] = message.clone().into();
    Ok(Report {
        json,
        text: format!("{message}\n"),
        code: 1,
    })
}
