//! Optional native-host JSON publication, between simulation ticks and independent of pause/speed.
use bevy::prelude::*;
use std::io::Write as _;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub(crate) struct ProjectionArgs {
    pub world_state_out: Option<PathBuf>,
    pub world_state_interval_ms: u64,
    interval_given: bool,
}
impl Default for ProjectionArgs {
    fn default() -> Self {
        Self {
            world_state_out: None,
            world_state_interval_ms: 500,
            interval_given: false,
        }
    }
}
impl ProjectionArgs {
    pub fn value(&mut self, flag: &str, value: &str) -> Option<Result<(), String>> {
        match flag {
            "--world-state-out" => {
                if value.is_empty() {
                    return Some(Err("--world-state-out needs a file path".into()));
                }
                self.world_state_out = Some(PathBuf::from(value));
                Some(Ok(()))
            }
            "--world-state-interval-ms" => {
                self.interval_given = true;
                Some(
                    value
                        .parse::<u64>()
                        .ok()
                        .filter(|value| (50..=60000).contains(value))
                        .map(|value| self.world_state_interval_ms = value)
                        .ok_or_else(|| "--world-state-interval-ms must be 50..=60000".into()),
                )
            }
            _ => None,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.interval_given && self.world_state_out.is_none() {
            return Err("--world-state-interval-ms requires --world-state-out".into());
        }
        if !(50..=60000).contains(&self.world_state_interval_ms) {
            return Err("--world-state-interval-ms must be 50..=60000".into());
        }
        if let Some(path) = &self.world_state_out
            && (path.file_name().is_none() || path.is_dir())
        {
            return Err("--world-state-out must name a file, not a directory".into());
        }
        Ok(())
    }
    /// Publish the constructed/resumed world before the first native frame. Unsupported scenes
    /// and unavailable destinations fail here, before the window's runner starts.
    pub fn install(&self, app: &mut App) -> Result<(), String> {
        self.validate()?;
        let Some(path) = &self.world_state_out else {
            return Ok(());
        };
        let mut publisher = Publisher {
            path: path.clone(),
            every: Duration::from_millis(self.world_state_interval_ms),
            last: Instant::now(),
            error: None,
        };
        publisher.write(app.world_mut())?;
        publisher.path = std::fs::canonicalize(&publisher.path)
            .map_err(|error| format!("cannot resolve published projection path: {error}"))?;
        let launch = island_content::explorer::host::ProjectionLaunch {
            output: publisher.path.clone(),
            interval_ms: self.world_state_interval_ms,
        };
        launch.validate()?;
        crate::explorer::retain_projection_launch(app.world_mut(), &launch)?;
        app.insert_resource(launch);
        app.insert_resource(publisher)
            .add_systems(PostUpdate, publish);
        Ok(())
    }
}

#[derive(Resource)]
struct Publisher {
    path: PathBuf,
    every: Duration,
    last: Instant,
    error: Option<String>,
}
impl Publisher {
    fn write(&mut self, world: &mut World) -> Result<(), String> {
        let value = island_content::projection::world_state(world)?;
        let bytes = serde_json::to_vec(&value).map_err(|error| error.to_string())?;
        let tmp = self.path.with_extension("projection.tmp");
        let result = (|| -> std::io::Result<()> {
            if let Some(parent) = self
                .path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            {
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
        result.map_err(|error| format!("cannot publish {}: {error}", self.path.display()))
    }
}
fn publish(world: &mut World) {
    if world.resource::<Publisher>().last.elapsed() < world.resource::<Publisher>().every {
        return;
    }
    world.resource_scope(|world, mut publisher: Mut<Publisher>| {
        publisher.last = Instant::now();
        match publisher.write(world) {
            Ok(()) => {
                if publisher.error.take().is_some() {
                    info!("world projection recovered");
                }
            }
            Err(error) => {
                if publisher.error.as_ref() != Some(&error) {
                    warn!("world projection: {error}");
                }
                publisher.error = Some(error);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use npc_framework::prelude::*;
    fn destination(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/native-projection-tests")
            .join(format!(
                "{tag}-{}-{}.json",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ))
    }
    fn ready(app: &mut App) {
        app.world_mut().resource_mut::<Publisher>().last =
            Instant::now() - Duration::from_millis(1000);
        app.world_mut().run_schedule(PostUpdate);
    }
    #[test]
    fn parser_defaults_are_opt_in_and_interval_requires_an_output() {
        let mut args = ProjectionArgs::default();
        assert!(args.world_state_out.is_none());
        assert_eq!(args.world_state_interval_ms, 500);
        args.validate().unwrap();
        assert!(args.value("--unrelated", "value").is_none());
        for value in ["49", "60001", "zero", "-1"] {
            assert!(
                args.value("--world-state-interval-ms", value)
                    .unwrap()
                    .is_err()
            );
        }
        args.value("--world-state-interval-ms", "50")
            .unwrap()
            .unwrap();
        assert!(args.validate().is_err());
        args.value("--world-state-out", "world.json")
            .unwrap()
            .unwrap();
        args.validate().unwrap();
        assert!(args.value("--world-state-out", "").unwrap().is_err());
    }
    #[test]
    fn native_host_publishes_initial_and_paused_post_update_without_changing_simulation() {
        let path = destination("paused");
        let mut app = island_content::new_sim_app(7, "ai_village");
        let args = ProjectionArgs {
            world_state_out: Some(path.clone()),
            ..default()
        };
        let before = npc_framework::kernel::hash::state_hash(app.world_mut());
        args.install(&mut app).unwrap();
        let descriptor = app
            .world()
            .resource::<island_content::explorer::host::ProjectionLaunch>();
        assert!(descriptor.output.is_absolute());
        assert_eq!(descriptor.output, std::fs::canonicalize(&path).unwrap());
        assert_eq!(descriptor.interval_ms, 500);
        assert_eq!(
            npc_framework::kernel::hash::state_hash(app.world_mut()),
            before
        );
        let read =
            || serde_json::from_slice::<serde_json::Value>(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(read()["tick"], 0);
        run_ticks(app.world_mut(), 10);
        app.world_mut().get_resource_or_init::<SimControl>().paused = true;
        let before = npc_framework::kernel::hash::state_hash(app.world_mut());
        ready(&mut app);
        assert_eq!(read()["tick"], 10);
        assert_eq!(read()["schema_version"], 1);
        assert_eq!(
            npc_framework::kernel::hash::state_hash(app.world_mut()),
            before
        );
        assert!(!path.with_extension("projection.tmp").exists());
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn initial_publication_uses_the_loaded_world_instead_of_a_fresh_tick_zero_world() {
        let path = destination("resume");
        let mut original = island_content::new_sim_app(7, "ai_village");
        run_ticks(original.world_mut(), 37);
        let snapshot = npc_framework::kernel::snapshot::save(original.world_mut()).unwrap();
        let mut resumed = island_content::new_sim_app(7, "ai_village");
        npc_framework::kernel::snapshot::load(
            resumed.world_mut(),
            &snapshot,
            npc_framework::kernel::snapshot::LoadMode::Strict,
        )
        .unwrap();
        let before = npc_framework::kernel::hash::state_hash(resumed.world_mut());
        ProjectionArgs {
            world_state_out: Some(path.clone()),
            ..default()
        }
        .install(&mut resumed)
        .unwrap();
        let published: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(published["tick"], 37);
        assert_eq!(
            npc_framework::kernel::hash::state_hash(resumed.world_mut()),
            before
        );
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn failed_periodic_publication_keeps_host_alive_and_recovers_after_destination_is_fixed() {
        let path = destination("recovery");
        let mut app = island_content::new_sim_app(7, "ai_village");
        ProjectionArgs {
            world_state_out: Some(path.clone()),
            ..default()
        }
        .install(&mut app)
        .unwrap();
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        let before = npc_framework::kernel::hash::state_hash(app.world_mut());
        ready(&mut app);
        assert!(app.world().resource::<Publisher>().error.is_some());
        assert!(!path.with_extension("projection.tmp").exists());
        std::fs::remove_dir(&path).unwrap();
        ready(&mut app);
        assert!(app.world().resource::<Publisher>().error.is_none());
        assert!(
            serde_json::from_slice::<serde_json::Value>(&std::fs::read(&path).unwrap()).is_ok()
        );
        assert_eq!(
            npc_framework::kernel::hash::state_hash(app.world_mut()),
            before
        );
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn unsupported_scene_and_bad_path_fail_before_export_is_installed_and_default_is_inert() {
        let path = destination("unsupported");
        let mut app = island_content::new_sim_app(1, "testbed");
        ProjectionArgs::default().install(&mut app).unwrap();
        assert!(!app.world().contains_resource::<Publisher>());
        let error = ProjectionArgs {
            world_state_out: Some(path.clone()),
            ..default()
        }
        .install(&mut app)
        .unwrap_err();
        assert!(error.contains("island village only"));
        assert!(!path.exists() && !app.world().contains_resource::<Publisher>());
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        assert!(
            ProjectionArgs {
                world_state_out: Some(directory),
                ..default()
            }
            .validate()
            .is_err()
        );
        let mut island = island_content::new_sim_app(1, "ai_village");
        let blocked = destination("blocked-parent");
        std::fs::create_dir_all(blocked.parent().unwrap()).unwrap();
        std::fs::write(&blocked, "parent is a file").unwrap();
        let nested = blocked.join("state.json");
        assert!(
            ProjectionArgs {
                world_state_out: Some(nested),
                ..default()
            }
            .install(&mut island)
            .is_err()
        );
        assert!(!island.world().contains_resource::<Publisher>());
        std::fs::remove_file(blocked).unwrap();
    }
}
