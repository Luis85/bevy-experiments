//! Standalone actor editor entry point; it never starts a simulation.
use std::path::PathBuf;

use bevy::prelude::*;
use npc_framework::view_helpers::{NpcViewHelpersPlugin, ScreenshotRequest};
use testbed_content::explorer::settings::Settings;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EditorArgs {
    pub actors_dir: PathBuf,
    pub initial: Option<String>,
    pub screenshot: Option<PathBuf>,
    pub after_secs: f32,
}

fn default_actors_dir() -> PathBuf {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_default();
    Settings::file(&|key| std::env::var(key).ok())
        .map(|file| Settings::load(&file, &home))
        .unwrap_or_else(|| Settings::default_for(&home))
        .actors_dir()
}

pub(crate) fn parse(values: &[String]) -> Result<Option<EditorArgs>, String> {
    if !values.iter().any(|s| s == "--actor-editor") {
        return Ok(None);
    }
    let mut args = EditorArgs {
        actors_dir: default_actors_dir(),
        initial: None,
        screenshot: None,
        after_secs: 5.0,
    };
    let mut iter = values.iter();
    while let Some(flag) = iter.next() {
        if flag == "--actor-editor" {
            continue;
        }
        if !matches!(
            flag.as_str(),
            "--actors-dir" | "--editor-preset" | "--screenshot" | "--after-secs"
        ) {
            return Err(format!("{flag} cannot be used with --actor-editor"));
        }
        let value = iter.next().ok_or_else(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--actors-dir" => {
                if value.is_empty() {
                    return Err("--actors-dir needs a folder".into());
                }
                args.actors_dir = PathBuf::from(value);
            }
            "--editor-preset" => args.initial = Some(value.clone()),
            "--screenshot" => args.screenshot = Some(PathBuf::from(value)),
            "--after-secs" => {
                args.after_secs = value
                    .parse()
                    .map_err(|_| "--after-secs needs a positive number")?;
                if !args.after_secs.is_finite() || args.after_secs <= 0.0 {
                    return Err("--after-secs needs a positive finite number".into());
                }
            }
            _ => unreachable!(),
        }
    }
    Ok(Some(args))
}

pub(crate) fn run(args: EditorArgs) -> Result<(), String> {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "npc-framework — Actor editor".into(),
                    resolution: (1600, 960).into(),
                    resize_constraints: bevy::window::WindowResizeConstraints {
                        min_width: 1120.0,
                        min_height: 720.0,
                        ..default()
                    },
                    ..default()
                }),
                close_when_requested: false,
                ..default()
            })
            .set(testbed_view::asset_plugin()),
    )
    .insert_resource(ClearColor(Color::srgb(0.04, 0.05, 0.06)))
    .add_plugins(NpcViewHelpersPlugin);
    crate::actor_editor::configure(&mut app, args.actors_dir);
    if let Some(id) = args.initial {
        crate::actor_editor::select_initial(&mut app, &id)?;
    }
    if let Some(path) = args.screenshot {
        app.insert_resource(ScreenshotRequest {
            path,
            after_secs: args.after_secs,
        });
    }
    app.run();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| (*s).into()).collect()
    }

    #[test]
    fn editor_mode_keeps_scenario_launches_unchanged() {
        assert_eq!(parse(&values(&["--scenario", "testbed"])).unwrap(), None);
    }

    #[test]
    fn editor_flags_select_the_folder_and_preview() {
        let args = parse(&values(&[
            "--actor-editor",
            "--actors-dir",
            "my actors",
            "--editor-preset",
            "rover",
            "--screenshot",
            "shot.png",
            "--after-secs",
            "2",
        ]))
        .unwrap()
        .unwrap();
        assert_eq!(args.actors_dir, PathBuf::from("my actors"));
        assert_eq!(args.initial.as_deref(), Some("rover"));
        assert_eq!(args.screenshot, Some(PathBuf::from("shot.png")));
        assert_eq!(args.after_secs, 2.0);
        for flags in [
            &["--actor-editor", "--actors-dir"][..],
            &["--actor-editor", "--scenario", "testbed"],
            &["--actor-editor", "--after-secs", "NaN"],
        ] {
            assert!(parse(&values(flags)).is_err());
        }
    }
}
