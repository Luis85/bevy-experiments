//! Optional host-side JSON publication. Never scheduled inside NpcSimTick.
use std::io::Write as _;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use bevy_ecs::world::World;
use clap::Args;
use npc_framework::tools::output::Failure;

#[derive(Args, Clone, Debug, Default)]
pub struct ProjectionArgs {
    /// Publish a full read-only world projection here (currently island village only).
    #[arg(long, value_name = "FILE")]
    pub world_state_out: Option<PathBuf>,
    /// Milliseconds between projections; host time, independent of simulation speed.
    #[arg(long, default_value_t = 500, value_parser = clap::value_parser!(u64).range(50..=60000), requires = "world_state_out")]
    pub world_state_interval_ms: u64,
}

pub struct Publisher {
    path: PathBuf,
    every: Duration,
    last: Instant,
    error: Option<String>,
}

impl ProjectionArgs {
    /// Publish tick zero before advertising the host, surfacing path/scene errors immediately.
    pub fn start(&self, world: &mut World) -> Result<Option<Publisher>, Failure> {
        let Some(path) = &self.world_state_out else {
            return Ok(None);
        };
        let mut publisher = Publisher {
            path: path.clone(),
            every: Duration::from_millis(self.world_state_interval_ms),
            last: Instant::now(),
            error: None,
        };
        publisher.write(world).map_err(Failure::usage)?;
        publisher.path = std::fs::canonicalize(&publisher.path).map_err(|error| {
            Failure::usage(format!("cannot resolve published projection path: {error}"))
        })?;
        let launch = testbed_content::explorer::host::ProjectionLaunch {
            output: publisher.path.clone(),
            interval_ms: self.world_state_interval_ms,
        };
        launch.validate().map_err(Failure::usage)?;
        world.insert_resource(launch);
        Ok(Some(publisher))
    }
}

impl Publisher {
    fn write(&mut self, world: &mut World) -> Result<(), String> {
        let value = testbed_content::projection::world_state(world)?;
        let bytes = serde_json::to_vec(&value).map_err(|e| e.to_string())?;
        let tmp = self.path.with_extension("projection.tmp");
        let result = (|| -> std::io::Result<()> {
            if let Some(parent) = self.path.parent().filter(|p| !p.as_os_str().is_empty()) {
                std::fs::create_dir_all(parent)?;
            }
            let mut file = std::fs::File::create(&tmp)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            std::fs::rename(&tmp, &self.path)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(tmp);
        }
        result.map_err(|e| format!("cannot publish {}: {e}", self.path.display()))
    }

    pub fn frame(&mut self, world: &mut World) {
        if self.last.elapsed() < self.every {
            return;
        }
        self.last = Instant::now();
        match self.write(world) {
            Ok(()) => {
                if self.error.take().is_some() {
                    tracing::info!("world projection recovered");
                }
            }
            Err(error) => {
                if self.error.as_ref() != Some(&error) {
                    tracing::warn!("world projection: {error}");
                }
                self.error = Some(error);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use npc_framework::prelude::*;

    #[test]
    fn export_publishes_initial_and_periodic_full_replacements_without_changing_hashes() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/projection-tests");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join(format!("island-{}.json", std::process::id()));
        let args = ProjectionArgs {
            world_state_out: Some(path.clone()),
            world_state_interval_ms: 50,
        };
        let mut app = testbed_content::new_sim_app(7, "ai_village");
        let mut publisher = args.start(app.world_mut()).unwrap().unwrap();
        let descriptor = app
            .world()
            .resource::<testbed_content::explorer::host::ProjectionLaunch>();
        assert!(descriptor.output.is_absolute());
        assert_eq!(descriptor.output, std::fs::canonicalize(&path).unwrap());
        assert_eq!(descriptor.interval_ms, 50);
        let read =
            || serde_json::from_slice::<serde_json::Value>(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(read()["tick"], 0);
        run_ticks(app.world_mut(), 10);
        let before = npc_framework::kernel::hash::state_hash(app.world_mut());
        publisher.last = Instant::now() - Duration::from_millis(100);
        publisher.frame(app.world_mut());
        assert_eq!(read()["tick"], 10);
        assert_eq!(
            npc_framework::kernel::hash::state_hash(app.world_mut()),
            before
        );
        assert!(!path.with_extension("projection.tmp").exists());
        std::fs::remove_file(path).unwrap();
    }
}
