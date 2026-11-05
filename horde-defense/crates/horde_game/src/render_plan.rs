//! The display plan: present mode, pipelined rendering and window mode, fixed
//! from the one startup settings read before the App exists, plus the one
//! system that applies display settings live.
//!
//! - Present mode and window mode apply live (`apply_display_settings`).
//! - Pipelined rendering is a plugin choice, so it applies only on restart.
//! - A `LASTLIGHT_PERF_PRESENT_MODE` or `LASTLIGHT_PERF_PIPELINED` value wins
//!   over settings and is recorded as the plan's source in the frame report.
use crate::{
    launch::{LaunchConfig, PerfOptions, PresentModeOverride},
    settings::{Settings, WindowModeSetting},
};
use bevy::{
    app::PluginGroupBuilder,
    prelude::*,
    render::pipelined_rendering::PipelinedRenderingPlugin,
    window::{MonitorSelection, PresentMode, PrimaryWindow, WindowMode, WindowResolution},
};
use std::path::Path;

/// Whether the render world runs pipelined on its own thread when neither
/// Low latency nor a launch override says otherwise. Stays `true` until the
/// measured decision (W1-61) records otherwise.
pub const PIPELINED_DEFAULT: bool = true;

/// Low latency gives Mailbox (Bevy falls back Mailbox → Immediate → Fifo
/// itself); VSync gives Fifo, today's default; neither gives AutoNoVsync.
pub fn present_mode_for(vsync: bool, low_latency: bool) -> PresentMode {
    if low_latency {
        PresentMode::Mailbox
    } else if vsync {
        PresentMode::Fifo
    } else {
        PresentMode::AutoNoVsync
    }
}

/// A launch override wins; otherwise Low latency turns pipelining off.
pub fn pipelined_for(low_latency: bool, perf_override: Option<bool>) -> bool {
    perf_override.unwrap_or(!low_latency && PIPELINED_DEFAULT)
}

/// The window mode a setting asks for on `monitor`.
pub fn window_mode_for(setting: WindowModeSetting, monitor: MonitorSelection) -> WindowMode {
    match setting {
        WindowModeSetting::Windowed => WindowMode::Windowed,
        WindowModeSetting::BorderlessFullscreen => WindowMode::BorderlessFullscreen(monitor),
    }
}

/// Where the plan's present mode and pipelined state came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanSource {
    /// The startup settings read.
    Settings,
    /// `LASTLIGHT_PERF_PRESENT_MODE` or `LASTLIGHT_PERF_PIPELINED` set part
    /// of the plan; settings then never change the present mode live.
    LaunchOverride,
}
impl PlanSource {
    /// The frame report's `present.source`.
    #[cfg(feature = "review")]
    pub fn label(self) -> &'static str {
        match self {
            Self::Settings => "settings",
            Self::LaunchOverride => "launch-override",
        }
    }
}

/// The display plan this process started with. Fixed per process: settings
/// edits change the window live, never this resource.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct RenderPlan {
    pub present_mode: PresentMode,
    pub pipelined: bool,
    /// The window mode the window is created with (primary monitor).
    pub window_mode: WindowMode,
    pub source: PlanSource,
}

/// The plan for `settings`; any perf override wins over them.
pub fn render_plan(settings: &Settings, perf: Option<&PerfOptions>) -> RenderPlan {
    let present_override = perf.and_then(|perf| perf.present_mode);
    let pipelined_override = perf.and_then(|perf| perf.pipelined);
    RenderPlan {
        present_mode: present_override.map_or_else(
            || present_mode_for(settings.vsync, settings.low_latency),
            PresentModeOverride::present_mode,
        ),
        pipelined: pipelined_for(settings.low_latency, pipelined_override),
        // A new window has no current monitor yet.
        window_mode: window_mode_for(settings.window_mode, MonitorSelection::Primary),
        source: if present_override.is_some() || pipelined_override.is_some() {
            PlanSource::LaunchOverride
        } else {
            PlanSource::Settings
        },
    }
}

/// The primary window: today's window plus the plan's present and window mode.
pub fn primary_window(config: &LaunchConfig, plan: &RenderPlan) -> Window {
    let (width, height) = config.window;
    Window {
        title: "Lastlight · Horde Defense".into(),
        visible: false,
        resolution: WindowResolution::new(width, height),
        mode: plan.window_mode,
        present_mode: plan.present_mode,
        // One queued frame instead of Bevy's two: the arena
        // aims at the pointer, so presentation latency is felt.
        desired_maximum_frame_latency: std::num::NonZero::new(1),
        resize_constraints: bevy::window::WindowResizeConstraints {
            min_width: 1050.0,
            min_height: 700.0,
            ..default()
        },
        ..default()
    }
}

/// `DefaultPlugins` as the game configures them, without pipelined rendering
/// when the plan says so.
pub fn default_plugins(
    config: &LaunchConfig,
    asset_directory: &Path,
    plan: &RenderPlan,
) -> PluginGroupBuilder {
    let plugins = DefaultPlugins
        .build()
        // `RUST_LOG`, when set, overrides this filter entirely.
        .set(bevy::log::LogPlugin {
            filter: crate::logging::DEFAULT_FILTER.into(),
            custom_layer: crate::logging::file_layer,
            ..default()
        })
        .set(AssetPlugin {
            file_path: asset_directory.to_string_lossy().into_owned(),
            ..default()
        })
        .set(WindowPlugin {
            primary_window: Some(primary_window(config, plan)),
            close_when_requested: false,
            ..default()
        });
    if plan.pipelined {
        plugins
    } else {
        plugins.disable::<PipelinedRenderingPlugin>()
    }
}

/// Inserts the plan and applies display settings live whenever they change.
pub struct DisplayPlugin(pub RenderPlan);
impl Plugin for DisplayPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.0.clone()).add_systems(
            Update,
            apply_display_settings.run_if(resource_changed::<Settings>),
        );
    }
}

/// Writes the primary window's mode and, unless a launch override fixed it,
/// its present mode, each only when it differs. Pipelined rendering never
/// changes here.
pub fn apply_display_settings(
    settings: Res<Settings>,
    plan: Res<RenderPlan>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    let Ok(mut window) = windows.single_mut() else {
        return;
    };
    // Borderless on any monitor already satisfies the setting.
    let mode = window_mode_for(settings.window_mode, MonitorSelection::Current);
    if std::mem::discriminant(&window.mode) != std::mem::discriminant(&mode) {
        window.mode = mode;
    }
    if plan.source == PlanSource::Settings {
        let present_mode = present_mode_for(settings.vsync, settings.low_latency);
        if window.present_mode != present_mode {
            window.present_mode = present_mode;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn perf(present_mode: Option<PresentModeOverride>, pipelined: Option<bool>) -> PerfOptions {
        PerfOptions {
            present_mode,
            pipelined,
            ..PerfOptions::default()
        }
    }
    fn settings(vsync: bool, low_latency: bool) -> Settings {
        Settings {
            vsync,
            low_latency,
            ..Settings::default()
        }
    }

    #[test]
    fn present_mode_matrix() {
        assert_eq!(present_mode_for(true, false), PresentMode::Fifo);
        assert_eq!(present_mode_for(false, false), PresentMode::AutoNoVsync);
        assert_eq!(present_mode_for(false, true), PresentMode::Mailbox);
        assert_eq!(present_mode_for(true, true), PresentMode::Mailbox);
        for (vsync, low_latency) in [(true, false), (false, false), (false, true), (true, true)] {
            let plan = render_plan(&settings(vsync, low_latency), None);
            assert_eq!(plan.present_mode, present_mode_for(vsync, low_latency));
            assert_eq!(plan.source, PlanSource::Settings);
        }
        // The override wins over every setting.
        for (setting, expected) in [
            (PresentModeOverride::Fifo, PresentMode::Fifo),
            (PresentModeOverride::Mailbox, PresentMode::Mailbox),
            (PresentModeOverride::Immediate, PresentMode::Immediate),
            (PresentModeOverride::AutoNoVsync, PresentMode::AutoNoVsync),
        ] {
            for low_latency in [false, true] {
                let plan = render_plan(
                    &settings(true, low_latency),
                    Some(&perf(Some(setting), None)),
                );
                assert_eq!(plan.present_mode, expected, "{setting:?}");
                assert_eq!(plan.source, PlanSource::LaunchOverride);
            }
        }
        // A perf launch without overrides still follows settings.
        let plan = render_plan(&settings(false, false), Some(&perf(None, None)));
        assert_eq!(plan.present_mode, PresentMode::AutoNoVsync);
        assert_eq!(plan.source, PlanSource::Settings);
        // The frame report's `present.source`.
        #[cfg(feature = "review")]
        assert_eq!(
            [
                PlanSource::Settings.label(),
                PlanSource::LaunchOverride.label()
            ],
            ["settings", "launch-override"]
        );
    }

    #[test]
    fn low_latency_disables_pipelined_unless_overridden() {
        assert_eq!(pipelined_for(false, None), PIPELINED_DEFAULT);
        assert!(!pipelined_for(true, None));
        assert!(pipelined_for(true, Some(true)));
        assert!(!pipelined_for(false, Some(false)));
        assert!(!render_plan(&settings(true, true), None).pipelined);
        let overridden = render_plan(&settings(true, true), Some(&perf(None, Some(true))));
        assert!(overridden.pipelined);
        assert_eq!(overridden.source, PlanSource::LaunchOverride);
        // Low latency still asks for Mailbox when only pipelining is overridden.
        assert_eq!(overridden.present_mode, PresentMode::Mailbox);
        let plugins = |plan: &RenderPlan| {
            default_plugins(&LaunchConfig::default(), Path::new("assets"), plan)
                .enabled::<PipelinedRenderingPlugin>()
        };
        assert!(plugins(&render_plan(&Settings::default(), None)));
        assert!(!plugins(&render_plan(&settings(true, true), None)));
        assert!(plugins(&overridden));
    }

    #[test]
    fn default_launch_is_fifo_pipelined_with_one_queued_frame() {
        let plan = render_plan(&Settings::default(), None);
        assert_eq!(
            plan,
            RenderPlan {
                present_mode: PresentMode::Fifo,
                pipelined: true,
                window_mode: WindowMode::Windowed,
                source: PlanSource::Settings,
            }
        );
        let window = primary_window(&LaunchConfig::default(), &plan);
        assert_eq!(window.present_mode, PresentMode::Fifo);
        assert_eq!(window.mode, WindowMode::Windowed);
        assert_eq!(
            window.desired_maximum_frame_latency.map(|n| n.get()),
            Some(1)
        );
        let fullscreen = render_plan(
            &Settings {
                window_mode: WindowModeSetting::BorderlessFullscreen,
                ..Settings::default()
            },
            None,
        );
        assert_eq!(
            primary_window(&LaunchConfig::default(), &fullscreen).mode,
            WindowMode::BorderlessFullscreen(MonitorSelection::Primary)
        );
    }

    fn display_app(plan: RenderPlan) -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(Settings::default())
            .add_plugins(DisplayPlugin(plan.clone()));
        app.world_mut().spawn((
            Window {
                present_mode: plan.present_mode,
                mode: plan.window_mode,
                ..default()
            },
            PrimaryWindow,
        ));
        app.update();
        app
    }
    fn window(app: &mut App) -> (PresentMode, WindowMode) {
        let mut query = app
            .world_mut()
            .query_filtered::<&Window, With<PrimaryWindow>>();
        let window = query.single(app.world()).unwrap();
        (window.present_mode, window.mode)
    }

    #[test]
    fn apply_display_settings_updates_window_live_but_not_pipelined() {
        let plan = render_plan(&Settings::default(), None);
        let mut app = display_app(plan.clone());
        assert_eq!(window(&mut app), (PresentMode::Fifo, WindowMode::Windowed));
        {
            let mut settings = app.world_mut().resource_mut::<Settings>();
            settings.vsync = false;
            settings.window_mode = WindowModeSetting::BorderlessFullscreen;
        }
        app.update();
        assert_eq!(
            window(&mut app),
            (
                PresentMode::AutoNoVsync,
                WindowMode::BorderlessFullscreen(MonitorSelection::Current)
            )
        );
        app.world_mut().resource_mut::<Settings>().low_latency = true;
        app.update();
        assert_eq!(window(&mut app).0, PresentMode::Mailbox);
        // Pipelined is a startup plugin choice: the plan never changes live.
        assert_eq!(*app.world().resource::<RenderPlan>(), plan);
        assert!(app.world().resource::<RenderPlan>().pipelined);
        app.world_mut().resource_mut::<Settings>().window_mode = WindowModeSetting::Windowed;
        app.update();
        assert_eq!(window(&mut app).1, WindowMode::Windowed);

        // A launch override fixes the present mode; window mode still applies.
        let plan = render_plan(
            &Settings::default(),
            Some(&perf(Some(PresentModeOverride::Immediate), None)),
        );
        let mut app = display_app(plan);
        {
            let mut settings = app.world_mut().resource_mut::<Settings>();
            settings.vsync = false;
            settings.window_mode = WindowModeSetting::BorderlessFullscreen;
        }
        app.update();
        assert_eq!(
            window(&mut app),
            (
                PresentMode::Immediate,
                WindowMode::BorderlessFullscreen(MonitorSelection::Current)
            )
        );
    }

    #[test]
    fn borderless_on_another_monitor_is_left_alone() {
        let plan = render_plan(
            &Settings {
                window_mode: WindowModeSetting::BorderlessFullscreen,
                ..Settings::default()
            },
            None,
        );
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(Settings {
                window_mode: WindowModeSetting::BorderlessFullscreen,
                ..Settings::default()
            })
            .add_plugins(DisplayPlugin(plan.clone()));
        app.world_mut().spawn((
            Window {
                mode: plan.window_mode,
                ..default()
            },
            PrimaryWindow,
        ));
        app.update();
        assert_eq!(
            window(&mut app).1,
            WindowMode::BorderlessFullscreen(MonitorSelection::Primary)
        );
    }
}
