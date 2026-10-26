//! The spawn panel: pick a unit, an owner and a heading, then click the ground to issue
//! `debug.spawn` there (ADR-0008).
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::input::EguiWantsInput;
use bevy_egui::{EguiContexts, egui};
use fl_math::{Angle, Fx, FxVec2};
use fl_protocol::{DebugSpawnCmd, PlayerId};

use crate::debug::console::issue_command;
use crate::driver::LocalDriver;
use crate::link::RunnerLink;
use crate::map_view::MapView;
use crate::terrain::ray_ground;
use crate::view::{ClientContent, LocalPlayer};

/// Where the cursor ray meets the ground (the terrain, or the plane `y = 0` without one);
/// `None` off the ground or without a cursor.
#[derive(Resource, Default)]
pub struct CursorGround(pub Option<Vec3>);

/// The spawn panel's choices.
#[derive(Resource)]
pub struct SpawnPanel {
    /// The unit type to place, by name.
    pub unit: Option<String>,
    pub owner: PlayerId,
    /// Whether a left click on the ground places a unit.
    pub placing: bool,
    /// The heading of placed units, in degrees counter-clockwise from +x.
    pub heading_deg: i32,
}

impl Default for SpawnPanel {
    fn default() -> Self {
        SpawnPanel {
            unit: None,
            owner: PlayerId(0),
            placing: false,
            heading_deg: 0,
        }
    }
}

impl SpawnPanel {
    /// The `debug.spawn` that places the chosen unit at world position `at`, if a unit is chosen.
    #[must_use]
    pub fn spawn_at(&self, at: Vec3) -> Option<DebugSpawnCmd> {
        let unit = self.unit.clone()?;
        let pos = FxVec2::new(
            Fx::from_f64(f64::from(at.x)).ok()?,
            Fx::from_f64(f64::from(at.z)).ok()?,
        );
        Some(DebugSpawnCmd {
            unit,
            owner: self.owner,
            pos,
            heading: Angle::from_degrees(Fx::from_int(self.heading_deg.rem_euclid(360))),
        })
    }
}

/// Casts the cursor ray from the first camera onto the ground into [`CursorGround`].
pub(crate) fn update_cursor_ground(
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    map: Option<Res<MapView>>,
    mut ground: ResMut<CursorGround>,
) {
    let point = window.and_then(|window| {
        let cursor = window.cursor_position()?;
        let (camera, transform) = cameras.iter().find(|(camera, _)| camera.is_active)?;
        let ray = camera.viewport_to_world(transform, cursor).ok()?;
        ray_ground(ray, map.as_deref().and_then(|map| map.terrain.as_ref()))
    });
    if ground.0 != point {
        ground.0 = point;
    }
}

/// While placing, a left click on the ground (not on egui) issues `debug.spawn` there as the
/// [`LocalPlayer`], for the panel's owner; a right click stops placing.
pub(crate) fn click_to_place(
    mouse: Res<ButtonInput<MouseButton>>,
    egui_input: Option<Res<EguiWantsInput>>,
    cursor: Res<CursorGround>,
    mut panel: ResMut<SpawnPanel>,
    link: Option<Res<RunnerLink>>,
    driver: Option<ResMut<LocalDriver>>,
    player: Option<Res<LocalPlayer>>,
) {
    if !panel.placing || egui_input.is_some_and(|e| e.wants_any_pointer_input()) {
        return;
    }
    if mouse.just_pressed(MouseButton::Right) {
        panel.placing = false;
        return;
    }
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    let (Some(at), Some(link), Some(mut driver)) = (cursor.0, link, driver) else {
        return;
    };
    let Some(payload) = panel.spawn_at(at) else {
        return;
    };
    let Some(LocalPlayer(player)) = player.as_deref().copied() else {
        tracing::warn!("spawn not issued: there is no local player");
        return;
    };
    if let Err(err) = issue_command(&link, &mut driver, player, Vec::new(), &payload) {
        tracing::warn!(%err, "spawn not issued");
    }
}

/// Draws the spawn panel.
pub(crate) fn spawn_panel_window(
    mut contexts: EguiContexts,
    mut panel: ResMut<SpawnPanel>,
    content: Option<Res<ClientContent>>,
    cursor: Res<CursorGround>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    egui::Window::new("Spawn")
        .default_open(false)
        .show(ctx, |ui| {
            let panel = &mut *panel;
            let selected = panel.unit.clone().unwrap_or_else(|| "(none)".to_owned());
            egui::ComboBox::from_label("unit")
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    for unit in content.iter().flat_map(|c| c.0.units()) {
                        ui.selectable_value(&mut panel.unit, Some(unit.id.clone()), &unit.id);
                    }
                });
            ui.horizontal(|ui| {
                ui.label("owner");
                ui.add(egui::DragValue::new(&mut panel.owner.0));
            });
            ui.add(egui::Slider::new(&mut panel.heading_deg, 0..=359).text("heading°"));
            let can_place = panel.unit.is_some();
            ui.add_enabled(
                can_place,
                egui::Checkbox::new(&mut panel.placing, "place on click"),
            );
            if panel.placing {
                ui.label("left click: place, right click: stop");
            }
            match cursor.0 {
                Some(at) => ui.label(format!("cursor ({:.1}, {:.1})", at.x, at.z)),
                None => ui.label("cursor off the ground"),
            };
        });
    Ok(())
}
