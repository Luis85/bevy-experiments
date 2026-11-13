//! Shape's "Export world": the host writes the shaped island as a world file
//! in the worlds folder. The simulation gives the `living_world` block
//! (`frontier::shape::export_world`); the rest of the document is the running
//! world's own rules, so the file loads like any world preset. Host data and
//! host files only: nothing here reaches the simulation.
use bevy::prelude::*;
use std::path::{Path, PathBuf};
use testbed_view::ToastTone;

/// Where exported worlds are written: the configured worlds folder.
#[derive(Resource, Clone, Debug)]
pub(crate) struct ExportFolder(pub(crate) PathBuf);

/// What the host toast's button means, when it belongs to an export: "Try
/// again" after a failed export, or "Start a new game on it" after one that
/// worked. Cleared whenever another host toast replaces it.
#[derive(Resource, Default)]
pub(crate) struct ExportState {
    pub(crate) retry: bool,
    /// The exported file the toast offers a new game on.
    pub(crate) offer: Option<PathBuf>,
    /// The world the New-game screen chooses once the island is saved.
    pub(crate) next_world: Option<PathBuf>,
}

/// Whether "Start a new game on it" is safe to offer now: this window owns
/// the save, the last save did not fail, and no exit is under way. The
/// button saves through the ordinary exit before anything is replaced, and
/// the current island is kept as a backup.
pub(crate) fn may_offer_new_game(world: &World) -> bool {
    let summary = crate::island::session::summary(world);
    summary.can_start
        && world
            .get_resource::<super::FrontendState>()
            .is_some_and(|flow| flow.pending.is_none())
        && crate::island::save_notice(world).is_none_or(|saved| !saved.failed)
}

pub(super) fn process(world: &mut World) {
    if testbed_view::take_world_export(world) {
        run(world);
    }
}

/// Export now and say so on the toast lane.
pub(super) fn run(world: &mut World) {
    match export(world) {
        Ok(path) => {
            let name = path
                .file_name()
                .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
            let text = format!("World exported as {name} in your worlds folder.");
            let offer = may_offer_new_game(world);
            {
                let mut state = world.resource_mut::<ExportState>();
                state.retry = false;
                state.offer = offer.then_some(path);
            }
            if offer {
                testbed_view::island_toast_with(
                    world,
                    ToastTone::Done,
                    text,
                    false,
                    "Start a new game on it",
                );
            } else {
                testbed_view::island_toast(world, ToastTone::Done, text, false, false);
            }
        }
        Err(error) => {
            {
                let mut state = world.resource_mut::<ExportState>();
                state.retry = true;
                state.offer = None;
            }
            testbed_view::island_toast(
                world,
                ToastTone::Failed,
                format!(
                    "The world could not be exported: {error}. Check the folder, then try again."
                ),
                true,
                true,
            );
        }
    }
}

/// A file name from a world's name: "the-living-island".
pub(crate) fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').to_owned();
    if out.is_empty() { "world".into() } else { out }
}

/// The first free `<slug>-shaped-N.json` in `folder`.
pub(crate) fn free_name(folder: &Path, slug: &str) -> Result<PathBuf, String> {
    (1..=999)
        .map(|n| folder.join(format!("{slug}-shaped-{n}.json")))
        .find(|path| !path.exists())
        .ok_or_else(|| "the folder already holds 999 shaped copies of this world".to_owned())
}

/// The world document: the running world's rules with the shaped
/// `living_world` block, checked like any world preset.
pub(crate) fn document(world: &World) -> Result<serde_json::Value, String> {
    let active = world
        .get_resource::<testbed_content::world_runtime::ActiveWorld>()
        .ok_or("this island has no world rules to export")?;
    let mut doc = serde_json::to_value(&active.0).map_err(|error| error.to_string())?;
    let shaped: serde_json::Value = serde_json::from_str(
        &testbed_content::village::frontier::shape::export_world(world),
    )
    .map_err(|error| error.to_string())?;
    let living = shaped
        .get("living_world")
        .filter(|block| !block.is_null())
        .cloned()
        .ok_or("only the living world can be exported")?;
    doc["living_world"] = living;
    // Shaping a shaped world again keeps one "(shaped)".
    let base = active.0.name.trim_end_matches(" (shaped)");
    doc["name"] = format!("{base} (shaped)").into();
    let preset: testbed_content::worlds::WorldPreset =
        serde_json::from_value(doc.clone()).map_err(|error| error.to_string())?;
    testbed_content::worlds::validate(&preset).map_err(|errors| errors.join("; "))?;
    Ok(doc)
}

/// Write the document to a new file; the path written.
pub(crate) fn export(world: &World) -> Result<PathBuf, String> {
    let folder = world
        .get_resource::<ExportFolder>()
        .map(|folder| folder.0.clone())
        .ok_or("no worlds folder is set")?;
    let doc = document(world)?;
    let name = doc["name"].as_str().unwrap_or("world").to_owned();
    std::fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
    let path = free_name(&folder, &slug(name.trim_end_matches(" (shaped)")))?;
    let text = serde_json::to_string_pretty(&doc).map_err(|error| error.to_string())?;
    // Written whole, then named: a half-written world never appears.
    let partial = path.with_extension("json.partial");
    std::fs::write(&partial, text).map_err(|error| error.to_string())?;
    std::fs::rename(&partial, &path).map_err(|error| {
        let _ = std::fs::remove_file(&partial);
        error.to_string()
    })?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shaped_world_is_written_as_a_valid_preset_beside_the_others() {
        let dir = std::env::temp_dir().join(format!("npc-export-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let rules = dir.join("rules").join("living_world.json");
        let path = dir.join("island.json");
        let mut app = crate::island::session::open(&path, None, Some(&rules), |_| {}).unwrap();
        app.insert_resource(ExportFolder(dir.join("rules")));
        crate::island::session::new_game(app.world_mut(), "Tamsin").unwrap();
        let first = export(app.world()).unwrap();
        let second = export(app.world()).unwrap();
        assert_ne!(first, second, "a second export takes the next free name");
        assert!(
            first
                .file_name()
                .unwrap()
                .to_string_lossy()
                .ends_with("-shaped-1.json")
        );
        let text = std::fs::read_to_string(&first).unwrap();
        let preset: testbed_content::worlds::WorldPreset = serde_json::from_str(&text).unwrap();
        testbed_content::worlds::validate(&preset).unwrap();
        assert!(preset.name.ends_with("(shaped)"));
        assert!(preset.living_world.is_some());
        drop(app);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_name_is_a_plain_slug() {
        assert_eq!(
            slug("The Living Island (shaped)"),
            "the-living-island-shaped"
        );
        assert_eq!(slug("  "), "world");
    }

    #[test]
    fn without_a_folder_the_export_says_so() {
        let world = World::new();
        assert_eq!(export(&world).unwrap_err(), "no worlds folder is set");
    }
}
