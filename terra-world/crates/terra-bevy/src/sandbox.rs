use crate::{PoliticalWorld, ResidentChunks, WorldConfig, simulation::WorldRuntime};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    fs::{self, File},
    io::Read,
    path::Path,
};
use terra_core::{GeoPosition, WorldClock};
use terra_history::PoliticsDataset;
use terra_sandbox::{Catalog, Command, SandboxState};

#[cfg(test)]
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Resource)]
pub struct SandboxRuntime {
    pub state: SandboxState,
    pub catalog: Catalog,
    pub player_id: u64,
    pub revision: u64,
    /// Local presentation can reconcile control once after a successful restore.
    /// This runtime serial is deliberately absent from durable world state.
    pub restore_revision: u64,
    pub last_message: String,
    pub dirty: bool,
}
impl SandboxRuntime {
    pub fn new(position: GeoPosition) -> Self {
        let mut state = SandboxState::default();
        state.add_player(1, position).expect("valid initial player");
        Self {
            state,
            catalog: Catalog::builtin(),
            player_id: 1,
            revision: 0,
            restore_revision: 0,
            dirty: false,
            last_message:
                "Claim land, gather resources, then build your first home. Tab focuses your player."
                    .into(),
        }
    }
}

/// Identity of the immutable input world. Saves from another era/provider are
/// rejected before any current game state is changed. Live inputs are mutable;
/// reproducible worlds should use a versioned historical/scenario pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldIdentity {
    pub base_data_revision: String,
    pub seed: u64,
    pub chunk_level: u8,
    pub terrain_resolution: usize,
    pub live_data: bool,
    pub osm_features: bool,
    pub data_epoch_unix: Option<i64>,
    pub political_dataset_revision: Option<String>,
}
impl From<&WorldConfig> for WorldIdentity {
    fn from(c: &WorldConfig) -> Self {
        Self {
            base_data_revision: c.base_data_revision.clone(),
            seed: c.seed,
            chunk_level: c.chunk_level,
            terrain_resolution: c.terrain_resolution,
            live_data: c.live_data,
            osm_features: c.osm_features,
            data_epoch_unix: c.data_epoch_unix,
            political_dataset_revision: c.political_dataset_revision.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalWorldSave {
    pub schema_version: u32,
    pub identity: WorldIdentity,
    pub clock: WorldClock,
    pub player_id: u64,
    pub state: SandboxState,
    pub politics: Option<PoliticsDataset>,
}
impl LocalWorldSave {
    pub fn validate_for(&self, config: &WorldConfig, catalog: &Catalog) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err("unsupported local world save version".into());
        }
        if self.identity != WorldIdentity::from(config) {
            return Err("save belongs to a different world, data revision, political dataset, or historical epoch".into());
        }
        if !self.clock.unix_seconds.is_finite()
            || !(-62135596800.0..=253402300799.0).contains(&self.clock.unix_seconds)
            || !self.clock.time_scale.is_finite()
            || !(0.0..=3600.0).contains(&self.clock.time_scale)
        {
            return Err("invalid saved simulation clock".into());
        }
        self.state.require_complete().map_err(|e| e.to_string())?;
        self.state
            .validate_with_catalog(catalog)
            .map_err(|e| e.to_string())?;
        if !self.state.players.contains_key(&self.player_id) {
            return Err("saved local player does not exist".into());
        }
        if self.politics.as_ref().map(|p| p.id.clone()) != self.identity.political_dataset_revision
        {
            return Err("saved political dataset identity differs".into());
        }
        if let Some(data) = &self.politics {
            data.validate().map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    pub fn write(&self, path: &Path) -> Result<(), String> {
        self.write_with_sync(path, terra_sandbox::publication::sync_directory)
    }
    /// Host-specific final namespace-flush boundary. A failure after publication
    /// leaves the new visible snapshot intact and returns an explicit error.
    pub fn write_with_sync(
        &self,
        path: &Path,
        sync: impl FnOnce(&Path) -> std::io::Result<()>,
    ) -> Result<(), String> {
        self.state.require_complete().map_err(|e| e.to_string())?;
        let bytes = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        if bytes.len() > 48 * 1024 * 1024 {
            return Err("world save exceeds 48 MiB limit".into());
        }
        terra_sandbox::publication::write_private_with_sync(
            path,
            &bytes,
            terra_sandbox::publication::PublishMode::Replace,
            sync,
        )
        .map_err(|error| error.to_string())
    }
    pub fn read(path: &Path, config: &WorldConfig, catalog: &Catalog) -> Result<Self, String> {
        let metadata = fs::metadata(path).map_err(|e| e.to_string())?;
        if !metadata.is_file() {
            return Err("world save must be a regular file".into());
        }
        if metadata.len() > 48 * 1024 * 1024 {
            return Err("world save exceeds 48 MiB limit".into());
        }
        let mut bytes = Vec::new();
        File::open(path)
            .map_err(|e| e.to_string())?
            .take(48 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > 48 * 1024 * 1024 {
            return Err("world save exceeds 48 MiB limit".into());
        }
        let save: Self = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        save.validate_for(config, catalog)?;
        Ok(save)
    }
}

pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    terra_sandbox::publication::write_private(
        path,
        bytes,
        terra_sandbox::publication::PublishMode::Replace,
    )
    .map_err(|error| error.to_string())
}

/// A single local consent review. Snapshot bytes are bounded and intentionally
/// have no Debug/serde representation: this is private ephemeral UI state.
pub struct ReviewedRestore {
    identity: WorldIdentity,
    player: u64,
    restore_revision: u64,
    save_path: std::path::PathBuf,
    region_path: Option<std::path::PathBuf>,
    generation: Option<u64>,
    bytes: Option<Vec<u8>>,
}
impl ReviewedRestore {
    pub fn capture(
        runtime: &WorldRuntime,
        sandbox: &SandboxRuntime,
        persistence: &crate::RegionalPersistence,
    ) -> Result<Self, String> {
        if runtime.network_authority || persistence.pending() || persistence.requires_reopen() {
            return Err(
                "Restore is unavailable while the host or storage worker owns the world.".into(),
            );
        }
        let generation = if persistence.enabled {
            Some(
                persistence
                    .status
                    .as_ref()
                    .filter(|s| s.generation > 0)
                    .ok_or("Create a checkpoint before reviewing restore.")?
                    .generation,
            )
        } else {
            None
        };
        let bytes = if generation.is_none() {
            Some(read_save_bytes(&runtime.config.save_path)?)
        } else {
            None
        };
        Ok(Self {
            identity: WorldIdentity::from(&runtime.config),
            player: sandbox.player_id,
            restore_revision: sandbox.restore_revision,
            save_path: runtime.config.save_path.clone(),
            region_path: runtime.config.region_store.clone(),
            generation,
            bytes,
        })
    }
    pub fn description(&self) -> String {
        match self.generation {
            Some(generation) => format!(
                "Regional checkpoint {generation} · {}",
                self.region_path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default()
            ),
            None => format!(
                "Saved snapshot · {} · {} bytes",
                self.save_path.display(),
                self.bytes.as_ref().map_or(0, Vec::len)
            ),
        }
    }
    fn validate(
        &self,
        runtime: &WorldRuntime,
        sandbox: &SandboxRuntime,
        persistence: &crate::RegionalPersistence,
    ) -> Result<(), String> {
        if runtime.network_authority
            || persistence.pending()
            || persistence.requires_reopen()
            || self.identity != WorldIdentity::from(&runtime.config)
            || self.player != sandbox.player_id
            || self.restore_revision != sandbox.restore_revision
            || self.save_path != runtime.config.save_path
            || self.region_path != runtime.config.region_store
            || self.generation.is_some() != persistence.enabled
            || self.generation
                != persistence
                    .status
                    .as_ref()
                    .filter(|_| persistence.enabled)
                    .map(|s| s.generation)
        {
            return Err("Restore context changed. Review the current checkpoint again.".into());
        }
        Ok(())
    }
}

fn read_save_bytes(path: &Path) -> Result<Vec<u8>, String> {
    let metadata = fs::metadata(path).map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.len() > 48 * 1024 * 1024 {
        return Err("Snapshot must be a regular file within the 48 MiB limit.".into());
    }
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|e| e.to_string())?
        .take(48 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 48 * 1024 * 1024 {
        return Err("Snapshot exceeds the 48 MiB limit.".into());
    }
    Ok(bytes)
}

pub enum SandboxIntent {
    Command(Command),
    MoveTo(GeoPosition),
    Save,
    Load,
    LoadReviewed(ReviewedRestore),
}
#[derive(Resource, Default)]
/// Native intents are scoped to the currently installed world. A successful
/// explicit restore retires the remaining queue before another action can run
/// against the restored principal or state. A failed restore is not a barrier.
pub struct SandboxQueue(pub VecDeque<SandboxIntent>);

/// Environment checks shared by the native placement preview and local command
/// consumer. Permission, inventory and authored-object overlap stay in the rules.
pub fn placement_error(
    command: &Command,
    sandbox: &SandboxRuntime,
    runtime: &WorldRuntime,
    resident: &ResidentChunks,
) -> Option<String> {
    let (position, radius, footprint) = match command {
        Command::Settlement {
            action: terra_sandbox::SettlementCommand::Found { center, .. },
        } => (*center, 0., false),
        Command::ClaimPlot { center, .. } => (*center, 0., false),
        Command::Build {
            position,
            blueprint,
        } => (
            *position,
            sandbox
                .catalog
                .blueprints
                .get(blueprint)
                .map(|b| b.radius_m)
                .unwrap_or(0.),
            true,
        ),
        Command::Plant { position, .. } => (*position, 1., true),
        _ => return None,
    };
    let Some(surface) =
        crate::environment::physical_surface(resident, position, &runtime.effective_weather())
    else {
        return Some("wait for terrain to load at the placement location".into());
    };
    if (position.altitude_m - surface.terrain_height_m).abs() > 3. {
        return Some("placement requires known support within 3 m of the ground".into());
    }
    if surface.water || surface.slope_degrees > 32. {
        return Some("choose dry ground with a slope below 32 degrees".into());
    }
    if footprint && crate::environment::source_building_at(resident, position, radius) {
        return Some("keep the footprint clear of existing map buildings".into());
    }
    if footprint
        && sandbox
            .state
            .living
            .facility_footprints()
            .iter()
            .any(|(p, r)| p.surface_distance_m(position) < radius + r)
    {
        return Some("keep the footprint clear of markets and production facilities".into());
    }
    if let Command::Build { blueprint, .. } = command {
        if let Some(definition) = sandbox.catalog.blueprints.get(blueprint) {
            let height = if definition.housing { 8. } else { 2.5 };
            match sandbox
                .state
                .construction_obstruction(position, radius, height)
            {
                Ok(Some(actor)) => {
                    return Some(format!(
                        "keep the building footprint clear of actor #{actor}; the entire body must be outside it"
                    ));
                }
                Err(error) => return Some(error.to_string()),
                Ok(None) => {}
            }
        }
    }
    None
}

fn vehicle_exit_ground(
    resident: &ResidentChunks,
    mut point: GeoPosition,
    weather: &terra_core::WeatherSample,
) -> Option<GeoPosition> {
    let surface = crate::environment::physical_surface(resident, point, weather)?;
    if surface.water || surface.slope_degrees > 44. {
        return None;
    }
    point.altitude_m = surface.terrain_height_m + 0.025;
    (!crate::environment::source_building_at_with_clearance(resident, point, 0.32, 1.8))
        .then_some(point)
}

fn garage_delivery_ground(
    resident: &ResidentChunks,
    mut point: GeoPosition,
    body_height_m: f64,
    weather: &terra_core::WeatherSample,
) -> Option<GeoPosition> {
    let surface = crate::environment::physical_surface(resident, point, weather)?;
    if surface.water || surface.slope_degrees > 32. {
        return None;
    }
    point.altitude_m = surface.terrain_height_m + 0.025;
    (!crate::environment::source_building_at_with_clearance(resident, point, 0.32, body_height_m))
        .then_some(point)
}

pub(crate) fn apply_sandbox_commands(
    mut sandbox: ResMut<SandboxRuntime>,
    mut queue: ResMut<SandboxQueue>,
    mut runtime: ResMut<WorldRuntime>,
    mut politics: ResMut<PoliticalWorld>,
    resident: Res<ResidentChunks>,
    mut ecology: Option<ResMut<crate::EcologyRuntime>>,
    mut living_diagnostics: Option<ResMut<crate::LivingDiagnostics>>,
    mut persistence: ResMut<crate::RegionalPersistence>,
) {
    // The network bridge is the only queue consumer in authoritative sessions.
    // UI systems may enqueue after its bounded send pass in this frame; leave
    // those late intents and any remaining backlog for the next network pass.
    if runtime.network_authority {
        return;
    }
    for _ in 0..16 {
        // Preserve queued intents while a submitted world revision is owned by
        // the storage worker. Resume only after its matching result installs.
        if persistence.pending() {
            break;
        }
        let Some(mut intent) = queue.0.pop_front() else {
            break;
        };
        let mut reviewed_generation = None;
        let mut reviewed_bytes = None;
        if let SandboxIntent::LoadReviewed(review) = intent {
            if let Err(error) = review.validate(&runtime, &sandbox, &persistence) {
                sandbox.last_message = error;
                continue;
            }
            reviewed_generation = review.generation;
            reviewed_bytes = review.bytes;
            intent = SandboxIntent::Load;
        }
        let restoring = matches!(intent, SandboxIntent::Load);
        let persistence_action =
            persistence.enabled && matches!(intent, SandboxIntent::Save | SandboxIntent::Load);
        if persistence.blocked() && !persistence_action {
            sandbox.last_message = if persistence.requires_reopen() {
                "World changes are paused after a storage worker failure. Close and reopen this world before retrying."
            } else {
                "World changes are paused after a persistence error. F5 retries the checkpoint; F9 restores the last committed world."
            }.into();
            continue;
        }
        // A preceding Save in this same queue can retire pages again. Hydrate
        // before each gameplay intent; an admitted activation remains open for
        // the batch, while every new activation validates committed storage.
        // Commands inspect distant ownership, workbenches and housing counts.
        if matches!(intent, SandboxIntent::Command(_) | SandboxIntent::MoveTo(_)) {
            match persistence.hydrate(&mut sandbox) {
                Ok(true) => persistence.residency_changed(),
                Ok(false) => {
                    queue.0.push_front(intent);
                    break;
                }
                Err(error) => {
                    persistence.fail(error.clone(), &mut runtime);
                    sandbox.last_message =
                        format!("Command activation failed. World paused: {error}");
                    continue;
                }
            }
        }
        let player = sandbox.player_id;
        let mut changed = false;
        let result: Result<String, String> = match intent {
            SandboxIntent::Command(command) => {
                if let Some(error) = placement_error(&command, &sandbox, &runtime, &resident) {
                    Err(error)
                } else {
                    let weather = runtime.effective_weather();
                    let SandboxRuntime { state, catalog, .. } = &mut *sandbox;
                    let outcome = match command {
                        Command::Living {
                            action:
                                terra_sandbox::LivingCommand::MoveUnits {
                                    unit_ids,
                                    destination,
                                },
                        } => state.move_units_with_environment(
                            player,
                            unit_ids,
                            destination,
                            catalog,
                            |kind, _, point| {
                                crate::living::ground_candidate(
                                    &resident,
                                    point,
                                    kind,
                                    &weather,
                                    true,
                                    &[],
                                )
                            },
                        ),
                        Command::Transport {
                            action: terra_sandbox::TransportCommand::ExitVehicle,
                        } => state.exit_vehicle_with_ground(player, catalog, |point| {
                            vehicle_exit_ground(&resident, point, &weather)
                        }),
                        Command::Transport {
                            action:
                                action @ (terra_sandbox::TransportCommand::BuyVehicle { .. }
                                | terra_sandbox::TransportCommand::RentVehicle { .. }),
                        } => state.deliver_vehicle_with_ground(
                            player,
                            action,
                            catalog,
                            |point, height| {
                                garage_delivery_ground(&resident, point, height, &weather)
                            },
                        ),
                        command => state.apply(player, command, catalog),
                    };
                    outcome
                        .map(|o| {
                            changed = true;
                            o.message
                        })
                        .map_err(|e| e.to_string())
                }
            }
            SandboxIntent::MoveTo(position) => {
                let result = if sandbox.state.living.enabled {
                    let unit = sandbox.state.living.active_unit(player).map(|u| u.id);
                    if let Some(unit_id) = unit {
                        let SandboxRuntime { state, catalog, .. } = &mut *sandbox;
                        state
                            .apply(
                                player,
                                Command::Living {
                                    action: terra_sandbox::LivingCommand::StopUnits {
                                        unit_ids: vec![unit_id],
                                    },
                                },
                                catalog,
                            )
                            .and_then(|_| {
                                state.set_controlled_motion(player, position, [0.; 3], 0.)
                            })
                    } else {
                        Err(terra_sandbox::SandboxError::PlayerMissing)
                    }
                } else {
                    sandbox.state.set_player_position(player, position)
                };
                result
                    .map(|_| {
                        changed = true;
                        "Active actor moved to the explorer focus.".into()
                    })
                    .map_err(|e| e.to_string())
            }
            SandboxIntent::Save if persistence.enabled => persistence.checkpoint(
                &mut sandbox,
                &mut runtime,
                &politics,
                &resident,
                "Explicit checkpoint",
            ),
            SandboxIntent::Load if persistence.enabled => persistence
                .restore_reviewed(
                    &mut sandbox,
                    &mut runtime,
                    &mut politics,
                    reviewed_generation,
                )
                .map(|message| {
                    if let Some(ecology) = ecology.as_mut() {
                        ecology.scheduler.reset();
                    }
                    if let Some(diagnostics) = living_diagnostics.as_mut() {
                        diagnostics.ecology_work = Default::default();
                        diagnostics.simulation_ms = 0.;
                        diagnostics.peak_simulation_ms = 0.;
                        diagnostics.fixed_steps = 0;
                        diagnostics.last_error = None;
                    }
                    message
                }),
            SandboxIntent::Save => {
                let save = LocalWorldSave {
                    schema_version: 1,
                    identity: WorldIdentity::from(&runtime.config),
                    clock: runtime.clock,
                    player_id: player,
                    state: sandbox.state.clone(),
                    politics: politics.dataset.clone(),
                };
                save.validate_for(&runtime.config, &sandbox.catalog)
                    .and_then(|_| save.write(&runtime.config.save_path))
                    .map(|_| {
                        sandbox.dirty = false;
                        format!(
                            "Saved world, factions, politics and clock to {}",
                            runtime.config.save_path.display()
                        )
                    })
            }
            SandboxIntent::LoadReviewed(_) => unreachable!("review normalized above"),
            SandboxIntent::Load => (|| {
                let bytes = read_save_bytes(&runtime.config.save_path)?;
                if reviewed_bytes
                    .as_ref()
                    .is_some_and(|expected| *expected != bytes)
                {
                    return Err("Saved snapshot changed. Review restore again.".into());
                }
                let save: LocalWorldSave =
                    serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
                save.validate_for(&runtime.config, &sandbox.catalog)?;
                Ok(save)
            })()
            .map(|save| {
                sandbox.state = save.state;
                if let Some(ecology) = ecology.as_mut() {
                    ecology.scheduler.reset();
                }
                if let Some(diagnostics) = living_diagnostics.as_mut() {
                    diagnostics.ecology_work = Default::default();
                    diagnostics.simulation_ms = 0.;
                    diagnostics.peak_simulation_ms = 0.;
                    diagnostics.fixed_steps = 0;
                    diagnostics.last_error = None;
                }
                sandbox.player_id = save.player_id;
                runtime.clock = save.clock;
                politics.dataset = save.politics;
                politics.draft.clear();
                politics.drawing = false;
                politics.revision += 1;
                sandbox.revision += 1;
                sandbox.restore_revision = sandbox.restore_revision.wrapping_add(1);
                sandbox.dirty = false;
                "World, factions, politics and clock restored. Tab focuses the saved player.".into()
            }),
        };
        match result {
            Ok(message) => {
                if persistence_action
                    && runtime
                        .last_error
                        .as_deref()
                        .is_some_and(|e| e.starts_with("Regional persistence:"))
                {
                    runtime.last_error = None;
                }
                sandbox.last_message = message;
                if changed {
                    sandbox.revision += 1;
                    sandbox.dirty = true;
                }
                if restoring {
                    // These intents were prepared against the previous world,
                    // including its principal, selected bodies and displayed
                    // financial quotes. A successful load replaces that entire
                    // context even when entity IDs and prices happen to match.
                    queue.0.clear();
                    break;
                }
            }
            Err(error) => {
                if persistence_action {
                    persistence.fail(error.clone(), &mut runtime);
                    sandbox.last_message =
                        format!("Regional operation failed. World paused: {error}");
                } else {
                    sandbox.last_message = format!("Action rejected: {error}");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn json_snapshot_publication_stays_private_under_permissive_umask() {
        use std::os::unix::fs::PermissionsExt;
        const CHILD: &str = "TERRA_PRIVATE_SNAPSHOT_CHILD";
        if let Some(directory) = std::env::var_os(CHILD) {
            let path = std::path::PathBuf::from(directory).join("snapshot.json");
            let config = WorldConfig::default();
            let sandbox = SandboxRuntime::new(config.focus);
            let mut save = LocalWorldSave {
                schema_version: 1,
                identity: WorldIdentity::from(&config),
                clock: WorldClock::default(),
                player_id: sandbox.player_id,
                state: sandbox.state,
                politics: None,
            };
            save.write(&path).unwrap();
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
            save.state
                .players
                .get_mut(&1)
                .unwrap()
                .inventory
                .insert("wood".into(), 71);
            save.write(&path).unwrap();
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
            let loaded = LocalWorldSave::read(&path, &config, &sandbox.catalog).unwrap();
            assert_eq!(
                serde_json::to_value(loaded).unwrap(),
                serde_json::to_value(save).unwrap()
            );
            assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
            return;
        }
        let directory = std::env::temp_dir().join(format!(
            "terra-private-snapshot-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        fs::create_dir(&directory).unwrap();
        // umask is process-global. Run the real save/read implementation in an
        // isolated test process rather than changing the parallel test runner.
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg("umask 000; exec \"$1\" --exact sandbox::tests::json_snapshot_publication_stays_private_under_permissive_umask --nocapture")
            .arg("terra-private-snapshot")
            .arg(std::env::current_exe().unwrap())
            .env(CHILD, &directory)
            .output()
            .unwrap();
        let _ = fs::remove_dir_all(directory);
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn failed_atomic_snapshot_publication_preserves_existing_output_and_cleans_temporary() {
        let directory = std::env::temp_dir().join(format!(
            "terra-failed-snapshot-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        fs::create_dir(&directory).unwrap();
        // A nonempty directory is an existing destination that rename cannot
        // replace. Its contents must survive after the fully written temp fails
        // publication, and that owned temporary must not be left behind.
        let destination = directory.join("snapshot.json");
        fs::create_dir(&destination).unwrap();
        fs::write(destination.join("original"), b"retained original bytes").unwrap();
        assert!(atomic_write(&destination, b"replacement").is_err());
        assert_eq!(
            fs::read(destination.join("original")).unwrap(),
            b"retained original bytes"
        );
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        fs::remove_dir_all(directory).unwrap();
    }

    fn restore_queue_fixture(regional: bool) -> (App, std::path::PathBuf, LocalWorldSave, Command) {
        use terra_core::{LocalFrame, WeatherSample};
        use terra_sandbox::LivingCommand;

        let directory = std::env::temp_dir().join(format!(
            "terra-restore-queue-{}-{}-{regional}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        let config = WorldConfig {
            focus: GeoPosition::new(48.137, 11.575, 100.).unwrap(),
            save_path: directory.join("world.json"),
            region_store: regional.then(|| directory.join("world.sqlite")),
            ..Default::default()
        };
        let mut sandbox = SandboxRuntime::new(config.focus);
        sandbox.state.add_player(2, config.focus).unwrap();
        sandbox
            .state
            .seed_living_demo(config.focus, &[], &sandbox.catalog)
            .unwrap();
        let shop = *sandbox.state.living.shops.keys().next().unwrap();
        let shop_position = sandbox.state.living.shops[&shop].position;
        for player in [1, 2] {
            let position = LocalFrame::new(shop_position)
                .to_geo([3. * player as f64, 0., 0.])
                .unwrap();
            sandbox.state.set_player_position(player, position).unwrap();
            let avatar = sandbox.state.living.accounts[&player].avatar_unit;
            sandbox
                .state
                .apply(
                    player,
                    Command::Living {
                        action: LivingCommand::Possess { unit_id: avatar },
                    },
                    &sandbox.catalog,
                )
                .unwrap();
        }
        let quote = sandbox
            .state
            .personal_trade_quote(1, shop, "bread", 1, true, &sandbox.catalog)
            .unwrap();
        let command = Command::Living {
            action: LivingCommand::TradeQuoted {
                shop_id: quote.shop_id,
                item: quote.item,
                amount: quote.amount,
                buy: quote.buy,
                quoted_total: quote.total_cost,
            },
        };
        let save = LocalWorldSave {
            schema_version: 1,
            identity: WorldIdentity::from(&config),
            clock: WorldClock::default(),
            player_id: 2,
            state: sandbox.state.clone(),
            politics: None,
        };
        save.validate_for(&config, &sandbox.catalog).unwrap();
        if regional {
            save.regional_archive(&config, &sandbox.catalog)
                .unwrap()
                .import_new(config.region_store.as_ref().unwrap())
                .unwrap();
        } else {
            save.write(&config.save_path).unwrap();
        }
        let runtime = WorldRuntime {
            focus: config.focus,
            frame: LocalFrame::new(config.focus),
            config,
            origin_revision: 0,
            clock: WorldClock::default(),
            weather: WeatherSample::default(),
            stream_stats: Default::default(),
            last_error: None,
            stream_poll_ms: 0.,
            simulation_ms: 0.,
            rebases: 0,
            network_authority: false,
        };
        let mut app = App::new();
        app.insert_resource(sandbox)
            .insert_resource(runtime)
            .insert_resource(PoliticalWorld::load(None).unwrap())
            .insert_resource(ResidentChunks {
                level: 15,
                chunks: Default::default(),
            })
            .init_resource::<crate::RegionalPersistence>()
            .init_resource::<SandboxQueue>()
            .add_message::<bevy::app::AppExit>()
            .add_systems(Startup, crate::regions::initialize_regions)
            .add_systems(Update, apply_sandbox_commands);
        app.update();
        assert!(
            !app.world()
                .resource::<crate::RegionalPersistence>()
                .blocked()
        );
        {
            let mut current = app.world_mut().resource_mut::<SandboxRuntime>();
            current.state = save.state.clone();
            current.player_id = 1;
            current.state.living.accounts.get_mut(&1).unwrap().money += 5_000;
            current.state.living.accounts.get_mut(&2).unwrap().money += 8_000;
        }
        (app, directory, save, command)
    }

    fn assert_restore_discards_old_world_actions(regional: bool) {
        use terra_sandbox::LivingCommand;
        let (mut app, directory, save, quoted_trade) = restore_queue_fixture(regional);
        let restored_before = app.world().resource::<SandboxRuntime>().restore_revision;
        // These commands are independently valid in the restored world, which
        // has the same shop/price but a different current principal. Ordinary
        // financial and ownership checks therefore cannot replace the barrier.
        let mut without_barrier = save.state.clone();
        let catalog = app.world().resource::<SandboxRuntime>().catalog.clone();
        without_barrier
            .apply(save.player_id, quoted_trade.clone(), &catalog)
            .unwrap();
        let release = Command::Living {
            action: LivingCommand::Release,
        };
        without_barrier
            .apply(save.player_id, release.clone(), &catalog)
            .unwrap();
        app.world_mut().resource_mut::<SandboxQueue>().0.extend([
            SandboxIntent::Load,
            SandboxIntent::Command(quoted_trade),
            SandboxIntent::Command(release),
        ]);
        app.update();
        let sandbox = app.world().resource::<SandboxRuntime>();
        assert_eq!(sandbox.player_id, 2);
        assert_eq!(sandbox.restore_revision, restored_before + 1);
        assert!(!sandbox.dirty);
        assert_eq!(sandbox.state.living.accounts, save.state.living.accounts);
        for player in [1, 2] {
            assert_eq!(
                sandbox.state.players[&player].inventory,
                save.state.players[&player].inventory,
            );
        }
        assert_eq!(sandbox.state.living.metrics, save.state.living.metrics);
        assert!(
            sandbox
                .last_message
                .to_ascii_lowercase()
                .contains("restored")
        );
        assert!(app.world().resource::<SandboxQueue>().0.is_empty());
        app.update();
        assert_eq!(
            app.world()
                .resource::<SandboxRuntime>()
                .state
                .living
                .accounts,
            save.state.living.accounts,
        );
        drop(app);
        let _ = fs::remove_dir_all(directory);
    }

    fn capture_review(app: &App) -> ReviewedRestore {
        ReviewedRestore::capture(
            app.world().resource::<WorldRuntime>(),
            app.world().resource::<SandboxRuntime>(),
            app.world().resource::<crate::RegionalPersistence>(),
        )
        .unwrap()
    }

    #[test]
    fn reviewed_restore_rejects_changed_context_and_exact_json_bytes_without_installing() {
        for case in 0..6 {
            let (mut app, directory, mut save, _) = restore_queue_fixture(false);
            let mut review = capture_review(&app);
            match case {
                0 => review.player += 1,
                1 => review.restore_revision += 1,
                2 => review.identity.seed += 1,
                3 => review.save_path = directory.join("other.json"),
                4 => review.region_path = Some(directory.join("other.sqlite")),
                _ => {
                    save.clock.unix_seconds += 1.;
                    save.write(&app.world().resource::<WorldRuntime>().config.save_path)
                        .unwrap();
                }
            }
            let before =
                serde_json::to_value(&app.world().resource::<SandboxRuntime>().state).unwrap();
            let revision = app.world().resource::<SandboxRuntime>().restore_revision;
            app.world_mut()
                .resource_mut::<SandboxQueue>()
                .0
                .push_back(SandboxIntent::LoadReviewed(review));
            app.update();
            assert_eq!(
                serde_json::to_value(&app.world().resource::<SandboxRuntime>().state).unwrap(),
                before,
                "case {case}"
            );
            assert_eq!(
                app.world().resource::<SandboxRuntime>().restore_revision,
                revision
            );
            assert!(
                app.world()
                    .resource::<SandboxRuntime>()
                    .last_message
                    .to_lowercase()
                    .contains("changed")
            );
            drop(app);
            let _ = fs::remove_dir_all(directory);
        }
    }

    #[test]
    fn reviewed_json_and_regional_restore_require_the_reviewed_target_and_retire_queue() {
        for regional in [false, true] {
            let (mut app, directory, save, command) = restore_queue_fixture(regional);
            // Regional fixture initialization already performs a startup restore;
            // consent must advance the installed context once from that baseline.
            let restored_before = app.world().resource::<SandboxRuntime>().restore_revision;
            let review = capture_review(&app);
            app.world_mut().resource_mut::<SandboxQueue>().0.extend([
                SandboxIntent::LoadReviewed(review),
                SandboxIntent::Command(command),
            ]);
            app.update();
            assert_eq!(
                app.world().resource::<SandboxRuntime>().player_id,
                save.player_id
            );
            assert_eq!(
                app.world()
                    .resource::<SandboxRuntime>()
                    .state
                    .living
                    .accounts,
                save.state.living.accounts
            );
            assert!(app.world().resource::<SandboxQueue>().0.is_empty());
            assert_eq!(
                app.world().resource::<SandboxRuntime>().restore_revision,
                restored_before + 1
            );
            drop(app);
            let _ = fs::remove_dir_all(directory);
        }
    }

    #[test]
    fn reviewed_regional_generation_is_checked_inside_the_store_before_installation() {
        let (mut app, directory, _, _) = restore_queue_fixture(true);
        // Startup has already restored this existing regional fixture once.
        // A rejected reviewed load must not advance that installed revision.
        let restored_before = app.world().resource::<SandboxRuntime>().restore_revision;
        let mut review = capture_review(&app);
        let actual = review.generation.unwrap();
        review.generation = Some(actual + 1);
        // Even if cached status agrees with the review, the actual transaction
        // candidate must match before store cursor or live state is installed.
        app.world_mut()
            .resource_mut::<crate::RegionalPersistence>()
            .status
            .as_mut()
            .unwrap()
            .generation = actual + 1;
        let before = serde_json::to_value(&app.world().resource::<SandboxRuntime>().state).unwrap();
        app.world_mut()
            .resource_mut::<SandboxQueue>()
            .0
            .push_back(SandboxIntent::LoadReviewed(review));
        app.update();
        assert_eq!(
            serde_json::to_value(&app.world().resource::<SandboxRuntime>().state).unwrap(),
            before
        );
        assert_eq!(
            app.world().resource::<SandboxRuntime>().restore_revision,
            restored_before
        );
        assert!(
            app.world()
                .resource::<SandboxRuntime>()
                .last_message
                .contains("Checkpoint changed")
        );
        drop(app);
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn successful_json_restore_retires_quoted_trade_and_control_from_previous_principal() {
        assert_restore_discards_old_world_actions(false);
    }

    #[test]
    fn successful_regional_restore_retires_quoted_trade_and_control_from_previous_principal() {
        assert_restore_discards_old_world_actions(true);
    }

    #[test]
    fn failed_json_restore_preserves_following_actions_for_the_current_principal() {
        use terra_sandbox::LivingCommand;
        let (mut app, directory, _, quoted_trade) = restore_queue_fixture(false);
        fs::remove_file(
            app.world()
                .resource::<WorldRuntime>()
                .config
                .save_path
                .clone(),
        )
        .unwrap();
        let before = app.world().resource::<SandboxRuntime>();
        let restored_before = before.restore_revision;
        let mut expected = before.state.clone();
        expected
            .apply(1, quoted_trade.clone(), &before.catalog)
            .unwrap();
        let release = Command::Living {
            action: LivingCommand::Release,
        };
        expected.apply(1, release.clone(), &before.catalog).unwrap();
        app.world_mut().resource_mut::<SandboxQueue>().0.extend([
            SandboxIntent::Load,
            SandboxIntent::Command(quoted_trade),
            SandboxIntent::Command(release),
        ]);
        app.update();
        let sandbox = app.world().resource::<SandboxRuntime>();
        assert_eq!(sandbox.player_id, 1);
        assert_eq!(sandbox.restore_revision, restored_before);
        assert_eq!(sandbox.state, expected);
        assert!(app.world().resource::<SandboxQueue>().0.is_empty());
        drop(app);
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn native_walking_dispatch_shortens_only_after_resident_support_is_available() {
        use std::{collections::BTreeMap, sync::Arc};
        use terra_core::{HeightField, LocalFrame, TileKey, WeatherSample, WorldChunk};
        use terra_sandbox::{LivingCommand, RoadRoute, RoadRules};

        let key = TileKey::at(WorldConfig::default().focus, 15).unwrap();
        let mut origin = key.center();
        origin.altitude_m = 100.025;
        let frame = LocalFrame::new(origin);
        let destination = frame.to_geo([50., 0., 0.]).unwrap();
        let mut sandbox = SandboxRuntime::new(origin);
        sandbox
            .state
            .seed_living_demo(origin, &[], &sandbox.catalog)
            .unwrap();
        let avatar = sandbox.state.living.accounts[&1].avatar_unit;
        sandbox.state.living.units.retain(|id, _| *id == avatar);
        sandbox
            .state
            .living
            .units
            .get_mut(&avatar)
            .unwrap()
            .position = origin;
        sandbox.state.living.shops.clear();
        sandbox.state.living.producers.clear();
        sandbox.state.living.deliveries.clear();
        sandbox.state.living.roads = vec![RoadRoute {
            id: "test:native-walking-perimeter".into(),
            points: [
                [-100., -80.],
                [100., -80.],
                [100., 100.],
                [-100., 100.],
                [-100., -80.],
            ]
            .into_iter()
            .map(|p| frame.to_geo([p[0], 0., p[1]]).unwrap())
            .collect(),
            width_m: 6.,
            rules: RoadRules::default(),
        }];
        let runtime = WorldRuntime {
            config: WorldConfig {
                focus: origin,
                ..Default::default()
            },
            focus: origin,
            frame,
            origin_revision: 0,
            clock: WorldClock::default(),
            weather: WeatherSample::default(),
            stream_stats: Default::default(),
            last_error: None,
            stream_poll_ms: 0.,
            simulation_ms: 0.,
            rebases: 0,
            network_authority: false,
        };
        let mut app = App::new();
        app.insert_resource(sandbox)
            .insert_resource(runtime)
            .insert_resource(PoliticalWorld::load(None).unwrap())
            .insert_resource(ResidentChunks {
                level: 15,
                chunks: BTreeMap::new(),
            })
            .init_resource::<crate::RegionalPersistence>()
            .init_resource::<SandboxQueue>()
            .add_systems(Update, apply_sandbox_commands);
        let intent = || {
            SandboxIntent::Command(Command::Living {
                action: LivingCommand::MoveUnits {
                    unit_ids: vec![avatar],
                    destination,
                },
            })
        };
        app.world_mut()
            .resource_mut::<SandboxQueue>()
            .0
            .push_back(intent());
        app.update();
        let state = &app.world().resource::<SandboxRuntime>().state;
        assert!(
            state.living.units[&avatar].path.len() > 1,
            "unknown terrain keeps the road route"
        );
        assert_eq!(state.living.units[&avatar].position, origin);

        app.world_mut()
            .resource_mut::<ResidentChunks>()
            .chunks
            .insert(
                key,
                Arc::new(WorldChunk {
                    key,
                    heights: HeightField::constant(100.),
                    features: vec![],
                    sources: vec![],
                    warnings: vec![],
                }),
            );
        app.world_mut()
            .resource_mut::<SandboxQueue>()
            .0
            .push_back(intent());
        app.update();
        let sandbox = app.world().resource::<SandboxRuntime>();
        let unit = &sandbox.state.living.units[&avatar];
        assert_eq!(
            unit.position, origin,
            "dispatch plans; the fixed step performs movement"
        );
        assert_eq!(unit.path.len(), 1);
        assert!(unit.path[0].surface_distance_m(destination) < 1e-6);
        assert!((unit.path[0].altitude_m - 100.025).abs() < 1e-6);
        assert_eq!(sandbox.state.living.accounts[&1].active_unit, avatar);
        sandbox
            .state
            .validate_with_catalog(&sandbox.catalog)
            .unwrap();
    }

    #[test]
    fn native_exit_dispatch_waits_for_source_then_places_the_avatar_clear() {
        use std::{collections::BTreeMap, sync::Arc};
        use terra_core::{HeightField, LocalFrame, TileKey, WeatherSample, WorldChunk};
        use terra_sandbox::TransportCommand;

        let key = TileKey::at(WorldConfig::default().focus, 15).unwrap();
        let mut origin = key.center();
        origin.altitude_m = 100.025;
        let frame = LocalFrame::new(origin);
        let seed = frame.to_geo([-250., 0., -250.]).unwrap();
        let mut sandbox = SandboxRuntime::new(seed);
        sandbox
            .state
            .seed_living_demo(seed, &[], &sandbox.catalog)
            .unwrap();
        sandbox
            .state
            .ensure_transport_demo(&sandbox.catalog)
            .unwrap();
        let vehicle = sandbox
            .state
            .transport
            .vehicles
            .values()
            .find(|v| v.owner == 1)
            .unwrap()
            .unit_id;
        sandbox
            .state
            .living
            .units
            .get_mut(&vehicle)
            .unwrap()
            .position = origin;
        sandbox.state.set_player_position(1, origin).unwrap();
        sandbox
            .state
            .apply(
                1,
                Command::Transport {
                    action: TransportCommand::EnterVehicle {
                        vehicle_id: vehicle,
                    },
                },
                &sandbox.catalog,
            )
            .unwrap();
        let before = sandbox.state.clone();
        let runtime = WorldRuntime {
            config: WorldConfig {
                focus: origin,
                ..Default::default()
            },
            focus: origin,
            frame,
            origin_revision: 0,
            clock: WorldClock::default(),
            weather: WeatherSample::default(),
            stream_stats: Default::default(),
            last_error: None,
            stream_poll_ms: 0.,
            simulation_ms: 0.,
            rebases: 0,
            network_authority: false,
        };
        let mut app = App::new();
        app.insert_resource(sandbox)
            .insert_resource(runtime)
            .insert_resource(PoliticalWorld::load(None).unwrap())
            .insert_resource(ResidentChunks {
                level: 15,
                chunks: BTreeMap::new(),
            })
            .init_resource::<crate::RegionalPersistence>()
            .init_resource::<SandboxQueue>()
            .add_systems(Update, apply_sandbox_commands);
        let exit = || {
            SandboxIntent::Command(Command::Transport {
                action: TransportCommand::ExitVehicle,
            })
        };
        app.world_mut()
            .resource_mut::<SandboxQueue>()
            .0
            .push_back(exit());
        app.update();
        assert_eq!(app.world().resource::<SandboxRuntime>().state, before);
        assert!(
            app.world()
                .resource::<SandboxRuntime>()
                .last_message
                .contains("supported ground")
        );
        app.world_mut()
            .resource_mut::<ResidentChunks>()
            .chunks
            .insert(
                key,
                Arc::new(WorldChunk {
                    key,
                    heights: HeightField::constant(100.),
                    features: vec![],
                    sources: vec![],
                    warnings: vec![],
                }),
            );
        app.world_mut()
            .resource_mut::<SandboxQueue>()
            .0
            .push_back(exit());
        app.update();
        let sandbox = app.world().resource::<SandboxRuntime>();
        let account = &sandbox.state.living.accounts[&1];
        let avatar = &sandbox.state.living.units[&account.avatar_unit];
        assert_eq!(account.active_unit, account.avatar_unit);
        assert_eq!(sandbox.state.spawn_obstruction(avatar).unwrap(), None);
        assert!((avatar.position.altitude_m - 100.025).abs() < 1e-6);
        assert!(avatar.position.surface_distance_m(origin) > 1.5);
        assert!(sandbox.state.transport.vehicles[&vehicle].parking_brake);
        assert_eq!(sandbox.revision, 1);

        let garage = sandbox
            .state
            .transport
            .facilities
            .values()
            .find(|f| f.kind == terra_sandbox::TransportFacilityKind::Garage)
            .unwrap()
            .id;
        let garage_position = sandbox.state.transport.facilities[&garage].position;
        app.world_mut()
            .resource_mut::<SandboxRuntime>()
            .state
            .set_player_position(1, garage_position)
            .unwrap();
        let before_delivery = app.world().resource::<SandboxRuntime>().state.clone();
        let chunks = std::mem::take(&mut app.world_mut().resource_mut::<ResidentChunks>().chunks);
        let buy = || {
            SandboxIntent::Command(Command::Transport {
                action: TransportCommand::BuyVehicle {
                    model: terra_sandbox::VehicleModel::Compact,
                    facility_id: garage,
                },
            })
        };
        app.world_mut()
            .resource_mut::<SandboxQueue>()
            .0
            .push_back(buy());
        app.update();
        assert_eq!(
            app.world().resource::<SandboxRuntime>().state,
            before_delivery
        );
        app.world_mut().resource_mut::<ResidentChunks>().chunks = chunks;
        app.world_mut()
            .resource_mut::<SandboxQueue>()
            .0
            .push_back(buy());
        app.update();
        let sandbox = app.world().resource::<SandboxRuntime>();
        let purchased = sandbox
            .state
            .transport
            .vehicles
            .keys()
            .find(|id| !before_delivery.transport.vehicles.contains_key(id))
            .copied()
            .unwrap();
        assert_eq!(
            sandbox
                .state
                .spawn_obstruction(&sandbox.state.living.units[&purchased])
                .unwrap(),
            None
        );
        assert!(
            (sandbox.state.living.units[&purchased].position.altitude_m - 100.025).abs() < 1e-6
        );
        app.world_mut()
            .resource_mut::<SandboxQueue>()
            .0
            .push_back(SandboxIntent::Command(Command::Transport {
                action: TransportCommand::EnterVehicle {
                    vehicle_id: purchased,
                },
            }));
        app.world_mut()
            .resource_mut::<SandboxQueue>()
            .0
            .push_back(exit());
        app.update();
        let sandbox = app.world().resource::<SandboxRuntime>();
        let account = &sandbox.state.living.accounts[&1];
        assert_eq!(account.active_unit, account.avatar_unit);
        assert_eq!(
            sandbox
                .state
                .spawn_obstruction(&sandbox.state.living.units[&account.avatar_unit])
                .unwrap(),
            None
        );
        assert_eq!(sandbox.revision, 4);
    }

    #[test]
    fn placement_feedback_uses_the_same_oriented_bus_footprint_as_authority() {
        use std::{collections::BTreeMap, sync::Arc};
        use terra_core::{HeightField, LocalFrame, TileKey, WeatherSample, WorldChunk};
        use terra_sandbox::VehicleModel;

        let key = TileKey::at(WorldConfig::default().focus, 15).unwrap();
        let mut origin = key.center();
        origin.altitude_m = 100.;
        let frame = LocalFrame::new(origin);
        let point = |east: f64, north: f64| {
            let mut point = frame.to_geo([east, 0., -north]).unwrap();
            point.altitude_m = 100.;
            point
        };
        let mut sandbox = SandboxRuntime::new(point(-250., -250.));
        sandbox
            .state
            .seed_living_demo(point(-250., -250.), &[], &sandbox.catalog)
            .unwrap();
        sandbox
            .state
            .ensure_transport_demo(&sandbox.catalog)
            .unwrap();
        let bus = sandbox
            .state
            .transport
            .vehicles
            .values()
            .find(|vehicle| vehicle.owner == 1)
            .unwrap()
            .unit_id;
        sandbox
            .state
            .transport
            .vehicles
            .get_mut(&bus)
            .unwrap()
            .model = VehicleModel::Bus;
        let unit = sandbox.state.living.units.get_mut(&bus).unwrap();
        unit.position = origin;
        unit.heading_deg = 90.;
        let config = WorldConfig {
            focus: origin,
            ..Default::default()
        };
        let runtime = WorldRuntime {
            config,
            focus: origin,
            frame,
            origin_revision: 0,
            clock: WorldClock::default(),
            weather: WeatherSample::default(),
            stream_stats: Default::default(),
            last_error: None,
            stream_poll_ms: 0.,
            simulation_ms: 0.,
            rebases: 0,
            network_authority: false,
        };
        let resident = ResidentChunks {
            level: 15,
            chunks: BTreeMap::from([(
                key,
                Arc::new(WorldChunk {
                    key,
                    heights: HeightField::constant(100.),
                    features: vec![],
                    sources: vec![],
                    warnings: vec![],
                }),
            )]),
        };
        let blocked = Command::Build {
            blueprint: "fence".into(),
            position: point(4., 0.),
        };
        let error = placement_error(&blocked, &sandbox, &runtime, &resident).unwrap();
        assert!(error.contains(&format!("actor #{bus}")), "{error}");
        let clear = Command::Build {
            blueprint: "workbench".into(),
            position: point(0., 3.),
        };
        assert_eq!(placement_error(&clear, &sandbox, &runtime, &resident), None);
    }

    #[test]
    fn local_save_keeps_clock_and_rejects_another_era() {
        let config = WorldConfig::default();
        let sandbox = SandboxRuntime::new(config.focus);
        let save = LocalWorldSave {
            schema_version: 1,
            identity: WorldIdentity::from(&config),
            clock: WorldClock {
                unix_seconds: 0.,
                time_scale: 10.,
                paused: true,
            },
            player_id: 1,
            state: sandbox.state,
            politics: None,
        };
        assert!(save.validate_for(&config, &sandbox.catalog).is_ok());
        let restored: LocalWorldSave =
            serde_json::from_slice(&serde_json::to_vec(&save).unwrap()).unwrap();
        assert_eq!(restored.clock, save.clock);
        let mut other = config;
        other.data_epoch_unix = Some(-2208988800);
        assert!(save.validate_for(&other, &sandbox.catalog).is_err());
    }
    #[test]
    fn invalid_saved_player_is_rejected() {
        let config = WorldConfig::default();
        let sandbox = SandboxRuntime::new(config.focus);
        let save = LocalWorldSave {
            schema_version: 1,
            identity: WorldIdentity::from(&config),
            clock: WorldClock::default(),
            player_id: 999,
            state: sandbox.state,
            politics: None,
        };
        assert!(save.validate_for(&config, &sandbox.catalog).is_err());
    }
}
