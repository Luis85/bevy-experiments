use crate::{
    PoliticalWorld, ResidentChunks, WorldConfig, WorldEnvironment, simulation::WorldRuntime,
};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use terra_core::{GeoPosition, WorldClock};
use terra_history::PoliticsDataset;
use terra_sandbox::{Catalog, Command, SandboxState};

#[derive(Resource)]
pub struct SandboxRuntime {
    pub state: SandboxState,
    pub catalog: Catalog,
    pub player_id: u64,
    pub revision: u64,
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
        let bytes = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        if bytes.len() > 48 * 1024 * 1024 {
            return Err("world save exceeds 48 MiB limit".into());
        }
        atomic_write(path, &bytes)
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
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let name = path
        .file_name()
        .ok_or("invalid output file name")?
        .to_string_lossy();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temp = parent.join(format!(".{name}.{}.{nonce}.tmp", std::process::id()));
    let result = (|| -> std::io::Result<()> {
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        fs::rename(&temp, path)?;
        if let Ok(dir) = File::open(parent) {
            let _ = dir.sync_all();
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result.map_err(|e| e.to_string())
}

pub enum SandboxIntent {
    Command(Command),
    MoveTo(GeoPosition),
    Save,
    Load,
}
#[derive(Resource, Default)]
pub struct SandboxQueue(pub VecDeque<SandboxIntent>);

pub(crate) fn apply_sandbox_commands(
    mut sandbox: ResMut<SandboxRuntime>,
    mut queue: ResMut<SandboxQueue>,
    mut runtime: ResMut<WorldRuntime>,
    mut politics: ResMut<PoliticalWorld>,
    resident: Res<ResidentChunks>,
) {
    // The network bridge is the only queue consumer in authoritative sessions.
    // UI systems may enqueue after its bounded send pass in this frame; leave
    // those late intents and any remaining backlog for the next network pass.
    if runtime.network_authority {
        return;
    }
    for _ in 0..16 {
        let Some(intent) = queue.0.pop_front() else {
            break;
        };
        let player = sandbox.player_id;
        let mut changed = false;
        let result: Result<String, String> = match intent {
            SandboxIntent::Command(command) => {
                let target = match &command {
                    Command::ClaimPlot { center, .. } => Some(*center),
                    Command::Build { position, .. } | Command::Plant { position, .. } => {
                        Some(*position)
                    }
                    _ => None,
                };
                if target.is_some_and(|p| {
                    resident
                        .terrain_height(p)
                        .is_none_or(|h| (p.altitude_m - h).abs() > 3.)
                }) {
                    Err("placement requires known terrain within 3 m of the ground".into())
                } else {
                    let SandboxRuntime { state, catalog, .. } = &mut *sandbox;
                    state
                        .apply(player, command, catalog)
                        .map(|o| {
                            changed = true;
                            o.message
                        })
                        .map_err(|e| e.to_string())
                }
            }
            SandboxIntent::MoveTo(position) => sandbox
                .state
                .set_player_position(player, position)
                .map(|_| {
                    changed = true;
                    "Player moved to the explorer focus.".into()
                })
                .map_err(|e| e.to_string()),
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
            SandboxIntent::Load => LocalWorldSave::read(
                &runtime.config.save_path,
                &runtime.config,
                &sandbox.catalog,
            )
            .map(|save| {
                sandbox.state = save.state;
                sandbox.player_id = save.player_id;
                runtime.clock = save.clock;
                politics.dataset = save.politics;
                politics.draft.clear();
                politics.drawing = false;
                politics.revision += 1;
                sandbox.revision += 1;
                sandbox.dirty = false;
                "World, factions, politics and clock restored. Tab focuses the saved player.".into()
            }),
        };
        match result {
            Ok(message) => {
                sandbox.last_message = message;
                if changed {
                    sandbox.revision += 1;
                    sandbox.dirty = true;
                }
            }
            Err(error) => sandbox.last_message = format!("Action rejected: {error}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
