//! [`load_world`] and [`load_world_with`]: a fresh sim from a save.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bevy_app::App;
use bevy_ecs::world::{Mut, World};
use fl_content::ContentDb;
use fl_protocol::{HeaderMismatch, PlayerCommand, PlayerId, SimId, StringTables, tables};

use super::format::{LoadCtx, SAVE_FORMAT_VERSION, SaveFile, SavedValue, SimSave};
use crate::build::{BuildError, MatchHeaderRes, MatchSetup, RunConfig, build_sim_app};
use crate::classify::{CodecError, FillFn, StateRegistry, from_postcard};
use crate::clock::SimClock;
use crate::commands::{last_seqs, remap_command_ids};
use crate::feature::SimFeature;
use crate::ids::{SimIdAllocator, spawn_with_id};

/// A loaded sim, ready to step from the save's tick.
pub struct Loaded {
    pub app: App,
    /// The commands that were stamped but not yet delivered when the save was made; hand them
    /// back to the driver ([`TestDriver::restore`](crate::TestDriver::restore)).
    pub pending: Vec<PlayerCommand>,
    /// The highest command seq the sim took from each player ([`LastSeq`](crate::LastSeq)),
    /// for the driver's command stamper.
    pub last_seq: BTreeMap<PlayerId, u32>,
    /// The saved units that were not loaded, in `SimId` order, with their unit type's name:
    /// the content has no such unit type ([`LoadOptions::drop_unknown_units`]).
    pub dropped: Vec<(SimId, String)>,
}

/// How lenient a load is. The default is strict.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LoadOptions {
    /// Skip every saved entity whose unit type (`"core.unit_type"`) names a unit the content
    /// lacks, instead of failing with [`LoadError::Codec`]: its `SimId` stays unused, and it
    /// is listed in [`Loaded::dropped`] and logged. The rebuild after a hot reload that
    /// removed a unit type loads this way (ADR-0012), so it needs a save with
    /// `cheats_allowed`: on any other save it is [`LoadError::DropNeedsCheats`], and a
    /// lockstep or late-join load can never drop units by accident.
    pub drop_unknown_units: bool,
}

/// Why a save did not load.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("the save does not decode: {0}")]
    Decode(String),
    #[error("save format version {found} is not supported (this build reads {supported})")]
    FormatVersion { found: u32, supported: u32 },
    #[error(transparent)]
    Header(HeaderMismatch),
    #[error("the save has component '{0}', which this sim does not save")]
    UnknownComponent(String),
    #[error("the save has resource '{0}', which this sim does not save")]
    UnknownResource(String),
    #[error("'{name}' was saved at version {from}; version {to} has no migration from it")]
    NoMigration { name: String, from: u32, to: u32 },
    #[error("'{name}' does not decode: {message}")]
    Codec { name: String, message: String },
    #[error("a pending command cannot be mapped to this sim: {0}")]
    Command(String),
    /// [`LoadOptions::drop_unknown_units`] on a save without `cheats_allowed`: dropping units
    /// is the testbed's hot-reload rebuild (ADR-0012), never a way to load a real match.
    #[error("dropping unknown units needs a save with cheats_allowed (a testbed rebuild)")]
    DropNeedsCheats,
    #[error(transparent)]
    Build(#[from] BuildError),
}

impl LoadError {
    fn codec(name: &str, err: CodecError) -> LoadError {
        match err {
            CodecError::NoMigration { from, to } => LoadError::NoMigration {
                name: name.to_owned(),
                from,
                to,
            },
            CodecError::Codec(message) => LoadError::Codec {
                name: name.to_owned(),
                message,
            },
        }
    }
}

/// [`load_world_with`] with the strict [`LoadOptions::default`].
///
/// # Errors
/// As [`load_world_with`].
pub fn load_world(
    features: &[Arc<dyn SimFeature>],
    content: Arc<ContentDb>,
    bytes: &[u8],
    run: RunConfig,
) -> Result<Loaded, LoadError> {
    load_world_with(features, content, bytes, run, LoadOptions::default())
}

/// Load the save `bytes` into a fresh sim of `features` and `content`:
/// 1. decode the [`SaveFile`], after checking its `format_version` against
///    [`SAVE_FORMAT_VERSION`];
/// 2. build the sim with the saved seed and cheats flag, and `run`;
/// 3. check the saved header with [`MatchHeader::check_save`](fl_protocol::MatchHeader::check_save)
///    (features and mods; the rules hash is not compared, so a rebuild after a content change
///    loads);
/// 4. spawn the entities in `SimId` order with their saved ids; with
///    [`LoadOptions::drop_unknown_units`] (on a save with `cheats_allowed` only), skip (and list
///    in [`Loaded::dropped`]) every unit whose saved unit type the content lacks;
/// 5. decode every resource and component by name, migrating older versions and remapping
///    dense ids from the save's string tables to the sim's; a `Hashed` resource the save
///    lacks is removed;
/// 6. set the [`SimIdAllocator`] and the [`SimClock`];
/// 7. fill the `Hashed` state names the save lacks whose codec has a
///    [`fill_missing`](crate::Codec::fill_missing), when the save's version of the owning
///    feature is older than the build's (R78): in feature load order, then by state name;
///    then check the terrain against the map and the content (a malformed `core.terrain` is
///    [`LoadError::Codec`], and so is a palette naming a terrain kind the content lacks:
///    `"unknown terrain '<id>'"`);
/// 8. run every feature's `Derived` rebuild
///    ([`SimBuilder::register_rebuild`](crate::SimBuilder::register_rebuild)), in feature
///    load order;
/// 9. re-plan the units whose flow field may be stale, with the value reload's helper: when
///    the save's rules hash is not the content's (always so for the ID-set rebuild with
///    [`LoadOptions::drop_unknown_units`]), every unit on a flow field, whose key may name a
///    profile or cluster layout the rebuilt graphs lack; otherwise one whose key's profile is
///    not its own. Under the content the save was made with, nothing changes. Only units on
///    a flow field are re-planned: a `Following` or `Waiting` unit keeps its corners or its
///    queued request (movement checks passability, and a queued request plans with the
///    unit's current profile);
/// 10. map the pending commands' type ids by name, from the save's `"commands"` table to
///     the sim's.
///
/// # Errors
/// The first problem found. Names the save has but this sim does not save are
/// [`LoadError::UnknownComponent`] and [`LoadError::UnknownResource`]; entities or names out
/// of order, duplicated, or with ids the allocator never issued are [`LoadError::Decode`]; a
/// pending command whose type name this sim lacks is [`LoadError::Command`]; a fill's error
/// is [`LoadError::Codec`] naming its state. Without
/// [`drop_unknown_units`](LoadOptions::drop_unknown_units), a unit whose type the content lacks
/// is [`LoadError::Codec`] for `"core.unit_type"`; with it, a save without `cheats_allowed`
/// is [`LoadError::DropNeedsCheats`].
pub fn load_world_with(
    features: &[Arc<dyn SimFeature>],
    content: Arc<ContentDb>,
    bytes: &[u8],
    run: RunConfig,
    opts: LoadOptions,
) -> Result<Loaded, LoadError> {
    let mut save = decode_save(bytes)?;
    check_order(&save.sim)?;
    if opts.drop_unknown_units && !save.header.cheats_allowed {
        return Err(LoadError::DropNeedsCheats);
    }

    let setup = MatchSetup {
        seed: save.header.seed,
        cheats_allowed: save.header.cheats_allowed,
    };
    let mut app = build_sim_app(features, content, setup, run)?;
    let current = app.world().resource::<MatchHeaderRes>().0.clone();
    current
        .check_save(&save.header)
        .map_err(LoadError::Header)?;

    let dropped = if opts.drop_unknown_units {
        drop_unknown_units(&mut save, &current.string_tables)
    } else {
        Vec::new()
    };
    let world = app.world_mut();
    let ctx = LoadCtx::new(&save.header.string_tables, &current.string_tables);
    let missing = world.resource_scope(|world, state: Mut<StateRegistry>| {
        restore(world, &state, &save.sim, &ctx)
    })?;
    world.insert_resource(SimIdAllocator::with_issued(save.sim.sim_id_next));
    world.insert_resource(SimClock {
        tick: save.sim.tick,
    });
    run_fills(world, missing, &save.header.features, &current.features)?;
    crate::terrain::check_loaded(world).map_err(|message| LoadError::Codec {
        name: "core.terrain".to_owned(),
        message,
    })?;
    rebuild_derived(world);
    crate::reload::replan_loaded_flow_units(world, save.header.rules_hash != current.rules_hash);

    let mut pending = save.sim.pending_commands;
    remap_command_ids(&ctx, &mut pending).map_err(LoadError::Command)?;
    let last_seq = last_seqs(world);
    Ok(Loaded {
        app,
        pending,
        last_seq,
        dropped,
    })
}

/// The `"core.unit_type"` state name and the version of its codec this reads.
const UNIT_TYPE: (&str, u32) = ("core.unit_type", 1);

/// Remove from `save` every entity whose saved unit type (`"core.unit_type"` v1: the dense id
/// in the save's [`tables::UNITS`]) names no unit in `current`'s, warning for each; returns
/// them, in `SimId` order, with the unit type's name. An entity whose unit type does not
/// decode stays, so the load reports it.
fn drop_unknown_units(save: &mut SaveFile, current: &StringTables) -> Vec<(SimId, String)> {
    let saved_units = save
        .header
        .string_tables
        .get(tables::UNITS)
        .unwrap_or_default();
    let unknown = |components: &[SavedValue]| -> Option<String> {
        let (_, _, bytes) = components
            .iter()
            .find(|(name, version, _)| (name.as_str(), *version) == UNIT_TYPE)?;
        let saved: u16 = from_postcard(bytes).ok()?;
        let name = saved_units.get(usize::from(saved))?;
        current
            .id_of(tables::UNITS, name)
            .is_none()
            .then(|| name.clone())
    };
    let mut dropped = Vec::new();
    save.sim
        .entities
        .retain(|(id, components)| match unknown(components) {
            Some(unit) => {
                tracing::warn!(%id, %unit, "the content has no such unit type: unit dropped");
                dropped.push((*id, unit));
                false
            }
            None => true,
        });
    dropped
}

/// The [`SaveFile`] in `bytes`, if its format version is ours.
fn decode_save(bytes: &[u8]) -> Result<SaveFile, LoadError> {
    let (found, _) = postcard::take_from_bytes::<u32>(bytes)
        .map_err(|err| LoadError::Decode(err.to_string()))?;
    if found != SAVE_FORMAT_VERSION {
        return Err(LoadError::FormatVersion {
            found,
            supported: SAVE_FORMAT_VERSION,
        });
    }
    from_postcard(bytes).map_err(LoadError::Decode)
}

/// Entities in strictly ascending `SimId` order, each issued by the allocator; resources
/// and each entity's components in strictly ascending name order.
fn check_order(sim: &SimSave) -> Result<(), LoadError> {
    fn ascending<'a>(what: &str, names: impl Iterator<Item = &'a str>) -> Result<(), LoadError> {
        let names: Vec<&str> = names.collect();
        match names.windows(2).find(|pair| pair[0] >= pair[1]) {
            Some(pair) => Err(LoadError::Decode(format!(
                "{what}: '{}' and '{}' are out of order or duplicated",
                pair[0], pair[1]
            ))),
            None => Ok(()),
        }
    }

    ascending(
        "resources",
        sim.resources.iter().map(|(n, _, _)| n.as_str()),
    )?;
    let mut previous = 0;
    for (id, components) in &sim.entities {
        if id.0 <= previous || id.0 > sim.sim_id_next {
            return Err(LoadError::Decode(format!(
                "entity {id} is out of order, duplicated or above the last issued id {}",
                sim.sim_id_next
            )));
        }
        previous = id.0;
        ascending(
            &format!("components of {id}"),
            components.iter().map(|(n, _, _)| n.as_str()),
        )?;
    }
    Ok(())
}

/// A `Hashed` state name the save lacks whose codec has a fill: `(feature, name, fill)`.
type Missing = (&'static str, &'static str, FillFn);

/// Steps 4 and 5 of [`load_world`]: the entities, then every resource and component. Returns
/// the state names the save lacks that have a fill.
fn restore(
    world: &mut World,
    state: &StateRegistry,
    sim: &SimSave,
    ctx: &LoadCtx<'_>,
) -> Result<Vec<Missing>, LoadError> {
    let mut missing = Vec::new();
    let entities: Vec<_> = sim
        .entities
        .iter()
        .map(|(id, components)| (spawn_with_id(world, *id, ()).id(), components))
        .collect();

    let mut saved = sim.resources.iter().peekable();
    for (_, name, entry) in state.resources.iter() {
        let Some(codec) = entry.codec.as_deref() else {
            continue;
        };
        match saved.next_if(|(saved_name, _, _)| saved_name.as_str() <= name) {
            Some((saved_name, version, bytes)) if saved_name == name => codec
                .load(world, *version, bytes, ctx)
                .map_err(|err| LoadError::codec(name, err))?,
            Some((saved_name, _, _)) => {
                return Err(LoadError::UnknownResource(saved_name.clone()));
            }
            None => {
                codec.remove(world);
                if let Some(fill) = codec.fill() {
                    missing.push((entry.feature, name, fill));
                }
            }
        }
    }
    if let Some((saved_name, _, _)) = saved.next() {
        return Err(LoadError::UnknownResource(saved_name.clone()));
    }

    let saved_components: BTreeSet<&str> = sim
        .entities
        .iter()
        .flat_map(|(_, components)| components.iter().map(|(name, _, _)| name.as_str()))
        .collect();
    for (_, name, entry) in state.components.iter() {
        let fill = entry.codec.as_deref().and_then(|codec| codec.fill());
        if let Some(fill) = fill
            && !saved_components.contains(name)
        {
            missing.push((entry.feature, name, fill));
        }
    }

    for (entity, components) in entities {
        for (name, version, bytes) in components {
            let codec = state
                .components
                .get(name)
                .and_then(|entry| entry.codec.as_deref())
                .ok_or_else(|| LoadError::UnknownComponent(name.clone()))?;
            codec
                .load(world, entity, *version, bytes, ctx)
                .map_err(|err| LoadError::codec(name, err))?;
        }
    }
    Ok(missing)
}

/// Step 7 of [`load_world`]: run the fill of every missing state name whose feature the save
/// recorded at an older version than `ours` (R78), in feature load order (`ours` is in it),
/// then by state name.
fn run_fills(
    world: &mut World,
    mut missing: Vec<Missing>,
    saved: &[(String, u32)],
    ours: &[(String, u32)],
) -> Result<(), LoadError> {
    let version = |features: &[(String, u32)], id: &str| {
        features
            .iter()
            .find(|(feature, _)| feature == id)
            .map(|&(_, version)| version)
    };
    let load_order = |id: &str| ours.iter().position(|(feature, _)| feature == id);
    missing.retain(|&(feature, _, _)| {
        matches!((version(saved, feature), version(ours, feature)), (Some(s), Some(o)) if s < o)
    });
    missing.sort_by_key(|&(feature, name, _)| (load_order(feature), name));
    for (_, name, fill) in missing {
        fill(world).map_err(|message| LoadError::Codec {
            name: name.to_owned(),
            message,
        })?;
    }
    Ok(())
}

/// Run every feature's `Derived` rebuild, in feature load order: a feature's derived state
/// may build on its dependencies'. A load and a value-only hot reload
/// ([`apply_content_values`](crate::apply_content_values)) both run it.
pub(crate) fn rebuild_derived(world: &mut World) {
    for rebuild in world.resource::<StateRegistry>().rebuilds() {
        rebuild(world);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::SimBuilder;
    use crate::classify::StateClass;
    use crate::feature::CoreFeature;

    /// R122: [`drop_unknown_units`] decodes `"core.unit_type"` itself, at version
    /// `UNIT_TYPE.1`. A codec bump must update that decoder too, so it fails here first.
    #[test]
    fn unit_type_codec_version_matches_the_lenient_decoder() {
        let mut builder = SimBuilder::new();
        builder.build_feature(&CoreFeature);
        let (app, _) = builder.finish();
        let state = app.world().resource::<StateRegistry>();
        let entry = state
            .components
            .get(UNIT_TYPE.0)
            .expect("core registers core.unit_type");
        match entry.class {
            StateClass::Hashed { version } => assert_eq!(
                version, UNIT_TYPE.1,
                "core.unit_type's codec moved to v{version}: update drop_unknown_units"
            ),
            other => panic!("core.unit_type is {other:?}, not Hashed"),
        }
    }
}
