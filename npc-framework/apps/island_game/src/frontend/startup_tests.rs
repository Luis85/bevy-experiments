use super::*;
use bevy::ui::InteractionDisabled;
use island_view::SceneReadiness;
use npc_framework::kernel::hash::state_hash;

fn fixture(tag: &str) -> (App, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("npc-scene-startup-{tag}-{}", std::process::id()));
    let path = dir.join("island.json");
    let mut app = session::open(&path, None, None, |app| {
        app.add_plugins(island_view::TestbedViewPlugin);
        install(app);
    })
    .unwrap();
    app.init_resource::<Messages<AppExit>>();
    app.insert_resource(SceneReadiness::default());
    ui::spawn(app.world_mut());
    (app, path)
}
fn remove_fixture(app: App, path: std::path::PathBuf) {
    drop(app);
    assert!(!path.exists(), "preparation never writes a world save");
    std::fs::remove_file(path.with_extension("world.lock")).unwrap();
    std::fs::remove_dir(path.parent().unwrap()).unwrap();
}
fn enabled(world: &mut World, action: fn(&Action) -> bool) -> bool {
    world
        .query::<(&Action, Has<InteractionDisabled>)>()
        .iter(world)
        .find(|(candidate, _)| action(candidate))
        .is_some_and(|(_, disabled)| !disabled)
}

#[test]
fn scene_startup_is_opaque_and_non_mutating_until_renderer_acknowledges() {
    let (mut app, path) = fixture("ready");
    let hash = state_hash(app.world_mut());
    assert!(!enabled(app.world_mut(), |a| matches!(a, Action::New)));
    assert!(enabled(app.world_mut(), |a| matches!(a, Action::Quit)));
    assert!(enabled(app.world_mut(), |a| matches!(a, Action::Controls)));
    let alpha = app
        .world_mut()
        .query_filtered::<&BackgroundColor, With<ui::ShellRoot>>()
        .single(app.world())
        .unwrap()
        .0
        .alpha();
    assert_eq!(alpha, 1.0);
    app.world_mut()
        .resource_mut::<FrontendState>()
        .requests
        .extend([Action::New, Action::Continue, Action::ContinueInspector]);
    process(app.world_mut());
    assert_eq!(
        app.world().resource::<FrontendState>().screen,
        Screen::Title
    );
    assert_eq!(app.world().resource::<SimClock>().tick, 0);
    assert_eq!(state_hash(app.world_mut()), hash);
    assert!(!path.exists());

    app.world_mut().resource_mut::<SceneReadiness>().ready = true;
    ui::refresh(app.world_mut());
    assert!(enabled(app.world_mut(), |a| matches!(a, Action::New)));
    let alpha = app
        .world_mut()
        .query_filtered::<&BackgroundColor, With<ui::ShellRoot>>()
        .single(app.world())
        .unwrap()
        .0
        .alpha();
    // Ready: the island shows through the title's gradient, with no flat dim.
    assert_eq!(alpha, 0.0);
    assert!(
        app.world_mut()
            .query_filtered::<(), (With<ui::ShellRoot>, With<bevy::ui::BackgroundGradient>)>()
            .single(app.world())
            .is_ok()
    );
    assert_eq!(state_hash(app.world_mut()), hash);
    app.world_mut()
        .resource_mut::<FrontendState>()
        .requests
        .push(Action::New);
    process(app.world_mut());
    assert_eq!(app.world().resource::<FrontendState>().screen, Screen::New);
    assert!(app.world().resource::<SimControl>().paused);
    remove_fixture(app, path);
}

#[test]
fn scene_startup_failure_keeps_exit_available_and_rejects_beginning_a_life() {
    let (mut app, path) = fixture("failure");
    app.world_mut().resource_mut::<SceneReadiness>().error =
        Some("diagnostic detail for the log".into());
    ui::refresh(app.world_mut());
    assert!(!enabled(app.world_mut(), |a| matches!(a, Action::New)));
    assert!(enabled(app.world_mut(), |a| matches!(a, Action::Quit)));
    assert!(
        app.world_mut()
            .query::<&Text>()
            .iter(app.world())
            .any(|text| text.0.contains("could not finish preparing"))
    );
    assert!(
        !app.world_mut()
            .query::<&Text>()
            .iter(app.world())
            .any(|text| text.0.contains("diagnostic detail"))
    );
    set_screen(app.world_mut(), Screen::New);
    let hash = state_hash(app.world_mut());
    app.world_mut()
        .resource_mut::<FrontendState>()
        .requests
        .extend([Action::Begin, Action::BeginInspector]);
    process(app.world_mut());
    assert_eq!(app.world().resource::<FrontendState>().screen, Screen::New);
    assert_eq!(state_hash(app.world_mut()), hash);
    app.world_mut()
        .resource_mut::<FrontendState>()
        .requests
        .push(Action::Quit);
    process(app.world_mut());
    assert!(!app.world().resource::<Messages<AppExit>>().is_empty());
    remove_fixture(app, path);
}

#[test]
fn headless_frontend_does_not_require_a_gpu_acknowledgement() {
    let (mut app, path) = fixture("headless");
    app.world_mut().remove_resource::<SceneReadiness>();
    assert!(scene_ready(app.world()));
    ui::refresh(app.world_mut());
    assert!(enabled(app.world_mut(), |a| matches!(a, Action::New)));
    remove_fixture(app, path);
}

/// Every title and pause control that has something to say carries its
/// tooltip, the two-line Continue too (it is built apart from the others).
#[test]
fn every_menu_control_with_words_to_add_has_its_tooltip() {
    use island_view::game_ui::tooltip::Tooltip;
    let (mut app, path) = fixture("tips");
    app.world_mut().resource_mut::<SceneReadiness>().ready = true;
    session::new_game(app.world_mut(), "Robin").unwrap();
    for screen in [Screen::Title, Screen::Pause, Screen::Settings] {
        set_screen(app.world_mut(), screen);
        ui::refresh(app.world_mut());
        let missing: Vec<Action> = app
            .world_mut()
            .query::<(&Action, Has<Tooltip>)>()
            .iter(app.world())
            .filter(|(action, tip)| ui::menu_tip(**action).is_some() && !tip)
            .map(|(action, _)| *action)
            .collect();
        assert!(missing.is_empty(), "{screen:?}: {missing:?}");
    }
    let continues = app
        .world_mut()
        .query::<&Action>()
        .iter(app.world())
        .filter(|action| **action == Action::Continue)
        .count();
    assert_eq!(continues, 0, "Settings shows no Continue");
    set_screen(app.world_mut(), Screen::Title);
    ui::refresh(app.world_mut());
    assert!(
        app.world_mut()
            .query::<(&Action, &Tooltip)>()
            .iter(app.world())
            .any(|(action, _)| *action == Action::Continue)
    );
    drop(app);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("world.lock"));
    let _ = std::fs::remove_file(path.with_extension("previous-1.json"));
    let _ = std::fs::remove_dir(path.parent().unwrap());
}
