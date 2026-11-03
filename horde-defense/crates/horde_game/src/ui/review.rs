//! Explicit isolated screenshot fixtures. These startup-only staged campaigns
//! are never claimed as balance proof or used by the ordinary user profile.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GestureFixture {
    Throws,
    Melee,
}

fn gesture_fixture(
    scene: Option<&str>,
    motion: bool,
    sequence: bool,
    practice: bool,
) -> Option<GestureFixture> {
    if !motion || !sequence || practice {
        return None;
    }
    match scene {
        Some("gesture-throws") => Some(GestureFixture::Throws),
        Some("gesture-melee") => Some(GestureFixture::Melee),
        _ => None,
    }
}

fn scheduled_gesture_input(
    kind: GestureFixture,
    frame: u32,
    attempt: &horde_core::sim::Attempt,
) -> Option<Input> {
    let lane = attempt.lane_count / 2;
    let aim = [attempt.width_yards * 0.6, f32::from(lane) + 0.5];
    let mut input = Input {
        aim,
        ..Default::default()
    };
    match (kind, frame) {
        (GestureFixture::Throws, 64) => {
            input.throw = Some(ThrowRequest {
                kind: OrdnanceKind::Grenade,
                cell: [(attempt.width_yards * 0.6).floor() as u8, lane],
            })
        }
        (GestureFixture::Throws, 96) => {
            input.throw = Some(ThrowRequest {
                kind: OrdnanceKind::Molotov,
                cell: [(attempt.width_yards * 0.7).floor() as u8, lane],
            })
        }
        (GestureFixture::Melee, 64) => input.melee = true,
        _ => return None,
    }
    Some(input)
}

impl Frontend {
    /// Capture-service timing bridge. Only explicit isolated gesture exports
    /// can enqueue these ordinary controls; fixed runtime/core owns all effects.
    pub(crate) fn schedule_gesture_review(&mut self, frame: u32) {
        let scene = crate::environment::string("LASTLIGHT_REVIEW_SCREEN");
        let Some(kind) = gesture_fixture(
            scene.as_deref(),
            crate::environment::flag("LASTLIGHT_REVIEW_MOTION"),
            crate::environment::path("LASTLIGHT_CAPTURE_SEQUENCE").is_some(),
            crate::environment::flag("LASTLIGHT_PRACTICE"),
        ) else {
            return;
        };
        let Some(attempt) = self
            .game
            .attempt
            .as_ref()
            .filter(|attempt| attempt.mode == Mode::Personal && !attempt.paused)
        else {
            return;
        };
        let Some(input) = scheduled_gesture_input(kind, frame, attempt) else {
            return;
        };
        self.aim = input.aim;
        self.pending.aim = input.aim;
        if input.melee {
            self.pending.melee = true;
            self.pending_attack_aims.melee = Some(input.aim);
        }
        if let Some(request) = input.throw {
            self.attack_selection = match request.kind {
                OrdnanceKind::Grenade => battle::AttackSelection::Grenade,
                OrdnanceKind::Molotov => battle::AttackSelection::Molotov,
            };
            self.pending.throw = Some(request);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LootCaptureMode {
    Disabled,
    Motion,
    Still,
}
fn select_loot_capture_mode(
    scene: Option<&str>,
    motion: bool,
    sequence: bool,
    still: bool,
    practice: bool,
) -> LootCaptureMode {
    if scene != Some("quality-loot") || practice {
        LootCaptureMode::Disabled
    } else if motion && sequence {
        LootCaptureMode::Motion
    } else if !motion && !sequence && still {
        LootCaptureMode::Still
    } else {
        LootCaptureMode::Disabled
    }
}
pub(super) fn reticle_capture_preview() -> bool {
    matches!(
        crate::environment::string("LASTLIGHT_REVIEW_SCREEN").as_deref(),
        Some("reticle-reload" | "damage" | "damage-4x")
    ) && !crate::environment::flag("LASTLIGHT_PRACTICE")
        && (crate::environment::path("LASTLIGHT_REVIEW_CAPTURE").is_some()
            || (crate::environment::flag("LASTLIGHT_REVIEW_MOTION")
                && crate::environment::path("LASTLIGHT_CAPTURE_SEQUENCE").is_some()))
}
pub(super) fn damage_capture_mode() -> LootCaptureMode {
    let scene = crate::environment::string("LASTLIGHT_REVIEW_SCREEN");
    let allowed = matches!(scene.as_deref(), Some("damage" | "damage-4x"));
    select_loot_capture_mode(
        if allowed { Some("quality-loot") } else { None },
        crate::environment::flag("LASTLIGHT_REVIEW_MOTION"),
        crate::environment::path("LASTLIGHT_CAPTURE_SEQUENCE").is_some(),
        crate::environment::path("LASTLIGHT_REVIEW_CAPTURE").is_some(),
        crate::environment::flag("LASTLIGHT_PRACTICE"),
    )
}
pub(super) fn loot_capture_mode() -> LootCaptureMode {
    select_loot_capture_mode(
        crate::environment::string("LASTLIGHT_REVIEW_SCREEN").as_deref(),
        crate::environment::flag("LASTLIGHT_REVIEW_MOTION"),
        crate::environment::path("LASTLIGHT_CAPTURE_SEQUENCE").is_some(),
        crate::environment::path("LASTLIGHT_REVIEW_CAPTURE").is_some(),
        crate::environment::flag("LASTLIGHT_PRACTICE"),
    )
}

/// Review-only presentation override. Persistent and ordinary profiles keep
/// their chosen preference, and no game clock or random state is inspected.
pub(super) fn apply_reduced_override(
    f: &mut Frontend,
    scenario_present: bool,
    practice: bool,
    force_reduced: bool,
) {
    if scenario_present && !practice && force_reduced {
        f.settings.reduced_effects = true;
    }
}

pub(super) fn model_workshop(root: &mut egui::Ui, f: &Frontend) -> bool {
    let scene = crate::environment::string("LASTLIGHT_REVIEW_SCREEN");
    if let Some(id) = scene
        .as_deref()
        .and_then(|scene| scene.strip_prefix("model-parity-"))
    {
        model_parity(root, f, id);
        return true;
    }
    if let Some(id) = scene
        .as_deref()
        .and_then(|scene| scene.strip_prefix("model-parts-"))
    {
        model_parts(root, f, id);
        return true;
    }
    if let Some(id) = scene
        .as_deref()
        .and_then(|scene| scene.strip_prefix("model-matrix-"))
    {
        model_matrix(root, id);
        return true;
    }
    if let Some(id) = scene
        .as_deref()
        .and_then(|scene| scene.strip_prefix("model-repairs-"))
    {
        model_repairs(root, id);
        return true;
    }
    if !matches!(
        scene.as_deref(),
        Some("model-workshop" | "model-comparison")
    ) {
        return false;
    }
    assert!(
        !crate::environment::flag("LASTLIGHT_PRACTICE"),
        "Model workshop is isolated review only"
    );
    assert!(
        crate::environment::path("LASTLIGHT_REVIEW_CAPTURE").is_some()
            || crate::environment::path("LASTLIGHT_CAPTURE_SEQUENCE").is_some(),
        "Model workshop requires explicit capture output"
    );
    egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(INK).inner_margin(16))
        .show(root, |ui| {
            let compare = scene.as_deref() == Some("model-comparison");
            ui.heading(if compare {
                "Still artwork and battle stance"
            } else {
                "Character model workshop"
            });
            let rect = ui.available_rect_before_wrap();
            if compare {
                crate::visual_assets::rig_comparison(ui.painter(), f.textures, rect);
            } else {
                crate::visual_assets::rig_review(ui.painter(), f.textures, rect);
            }
        });
    true
}

#[derive(Clone, Debug, serde::Serialize)]
pub(crate) struct NativeParityComparison {
    background: [u8; 4],
    /// Physical screenshot coordinates: x, y, width, height.
    reference: [u32; 4],
    assembled: [u32; 4],
}

#[derive(Clone, Debug, serde::Serialize)]
pub(crate) struct NativeParityLayout {
    schema_version: u32,
    model_id: String,
    source_sha256: String,
    source_frame: [u32; 4],
    source_frame_note: String,
    pixels_per_point: f32,
    bind_audit: crate::player_model::verification::BindAudit,
    comparisons: Vec<NativeParityComparison>,
}

fn parity_layout_id() -> egui::Id {
    egui::Id::new("lastlight.player_model.native_parity_layout.v1")
}

/// Read by the existing screenshot observer, which owns artifact I/O. Drawing
/// the diagnostic only publishes transient presentation metadata in egui.
pub(super) fn parity_layout(ctx: &egui::Context) -> Option<NativeParityLayout> {
    ctx.data(|data| data.get_temp(parity_layout_id()))
}

fn model_parity(root: &egui::Ui, f: &Frontend, id: &str) {
    assert!(
        !crate::environment::flag("LASTLIGHT_PRACTICE"),
        "Model parity is isolated review only"
    );
    assert!(
        crate::environment::path("LASTLIGHT_REVIEW_CAPTURE").is_some(),
        "Model parity requires an explicit native screenshot"
    );
    let ctx = root.ctx();
    ctx.data_mut(|data| data.remove::<NativeParityLayout>(parity_layout_id()));
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("lastlight_model_parity"),
    ));
    let viewport = ctx.content_rect();
    painter.rect_filled(viewport, 0., INK);
    let result = paint_model_parity(&painter, f, id, viewport, ctx.pixels_per_point());
    match result {
        Ok(layout) => {
            ctx.data_mut(|data| data.insert_temp(parity_layout_id(), layout));
        }
        Err(error) => {
            painter.text(
                viewport.center(),
                egui::Align2::CENTER_CENTER,
                format!("Native model parity unavailable\n{error}"),
                egui::FontId::monospace(16.),
                egui::Color32::LIGHT_RED,
            );
        }
    }
}

fn paint_model_parity(
    painter: &egui::Painter,
    f: &Frontend,
    id: &str,
    viewport: egui::Rect,
    pixels_per_point: f32,
) -> Result<NativeParityLayout, String> {
    use crate::player_model::{
        ModelPlacement, builtin, draw_egui, resolve_model, verification::audit_bind,
    };
    let catalogue = builtin()?;
    let model = catalogue
        .model(id)
        .ok_or_else(|| format!("Unknown model {id}"))?;
    let source = catalogue
        .source(&model.definition.source.id)
        .ok_or("Missing compiled original source")?;
    let texture = f
        .textures
        .menu_idle
        .ok_or("Original managed texture is still loading")?;
    let source_frame = model.definition.source.layout_frame;
    let width = source_frame[2] - source_frame[0];
    let height = source_frame[3] - source_frame[1];
    let left = (viewport.left() * pixels_per_point).ceil() as u32 + 12;
    let top = (viewport.top() * pixels_per_point).ceil() as u32 + 52;
    if left + width * 4 + 12 * 3 > (viewport.right() * pixels_per_point).floor() as u32
        || top + height > (viewport.bottom() * pixels_per_point).floor() as u32
    {
        return Err("Source-resolution panels do not fit. Use LASTLIGHT_REVIEW_SIZE=1800x1000; no downscaling is allowed.".into());
    }
    let scale = 1. / pixels_per_point;
    let physical_rect = |column: u32| [left + column * (width + 12), top, width, height];
    let logical_rect = |rect: [u32; 4]| {
        egui::Rect::from_min_size(
            egui::pos2(rect[0] as f32 * scale, rect[1] as f32 * scale),
            egui::vec2(rect[2] as f32 * scale, rect[3] as f32 * scale),
        )
    };
    let texture_bindings = std::collections::BTreeMap::from([(source.id.clone(), texture)]);
    let identity = resolve_model(
        catalogue,
        id,
        ModelPlacement {
            source_origin: model.definition.ground,
            scale: 1.,
        },
    )?;
    let audit_id = egui::Id::new(("lastlight.model_bind_audit.v1", id, &source.rgba_fnv1a64));
    let bind_audit = if let Some(audit) = painter
        .ctx()
        .data(|data| data.get_temp::<crate::player_model::verification::BindAudit>(audit_id))
    {
        audit
    } else {
        let audit = audit_bind(source, model.definition.source.tile, &identity)?;
        painter
            .ctx()
            .data_mut(|data| data.insert_temp(audit_id, audit.clone()));
        audit
    };
    let source_uv = egui::Rect::from_min_max(
        egui::pos2(
            source_frame[0] as f32 / source.width as f32,
            source_frame[1] as f32 / source.height as f32,
        ),
        egui::pos2(
            source_frame[2] as f32 / source.width as f32,
            source_frame[3] as f32 / source.height as f32,
        ),
    );
    let mut comparisons = Vec::with_capacity(2);
    for (index, background) in [[0, 0, 0, 255], [255, 255, 255, 255]]
        .into_iter()
        .enumerate()
    {
        let reference = physical_rect(index as u32 * 2);
        let assembled = physical_rect(index as u32 * 2 + 1);
        for (rect, caption) in [
            (reference, "Original source"),
            (assembled, "Actual assembled bind"),
        ] {
            let rect = logical_rect(rect);
            painter.rect_filled(
                rect,
                0.,
                egui::Color32::from_rgba_unmultiplied(
                    background[0],
                    background[1],
                    background[2],
                    background[3],
                ),
            );
            painter.text(
                rect.left_top() - egui::vec2(0., 8. * scale),
                egui::Align2::LEFT_BOTTOM,
                caption,
                egui::FontId::monospace(13. * scale),
                egui::Color32::WHITE,
            );
        }
        let reference_painter = painter.with_clip_rect(logical_rect(reference));
        reference_painter.image(
            texture,
            logical_rect(reference),
            source_uv,
            egui::Color32::WHITE,
        );
        let assembled_rect = logical_rect(assembled);
        let placement = ModelPlacement {
            source_origin: [
                assembled_rect.left()
                    + (model.definition.ground[0] - source_frame[0] as f32) * scale,
                assembled_rect.top()
                    + (model.definition.ground[1] - source_frame[1] as f32) * scale,
            ],
            scale,
        };
        let assembled_pose = resolve_model(catalogue, id, placement)?;
        draw_egui(
            &painter.with_clip_rect(assembled_rect),
            &assembled_pose,
            &texture_bindings,
        )?;
        comparisons.push(NativeParityComparison {
            background,
            reference,
            assembled,
        });
    }
    painter.text(egui::pos2(left as f32 * scale, 12. * scale), egui::Align2::LEFT_TOP,
        format!("{id} · source 1:1 physical pixels · identical managed PMA/NEAREST · raw bind exact: {} · {} parts / {} rectangles", bind_audit.is_exact(), identity.packets.len(), bind_audit.rectangles),
        egui::FontId::monospace(14. * scale), egui::Color32::WHITE);
    Ok(NativeParityLayout {
        schema_version: 1,
        model_id: id.into(),
        source_sha256: source.png_sha256.clone(),
        source_frame,
        source_frame_note: if id == "rook" {
            "Legacy right edge 652 extended to 654 to retain the original faint pistol fringe; first exact native fixture, no screenshot baseline replaced.".into()
        } else {
            "Original authored June layout frame retained.".into()
        },
        pixels_per_point,
        bind_audit,
        comparisons,
    })
}

/// Human semantic inspection complements whole-image parity: every thumbnail
/// is the production packet renderer with exactly one actual part selected.
fn model_parts(root: &egui::Ui, f: &Frontend, id: &str) {
    assert!(
        !crate::environment::flag("LASTLIGHT_PRACTICE"),
        "Model parts are isolated review only"
    );
    assert!(
        crate::environment::path("LASTLIGHT_REVIEW_CAPTURE").is_some(),
        "Model parts require explicit native capture"
    );
    let painter = root.ctx().layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("lastlight_model_parts"),
    ));
    let viewport = root.ctx().content_rect();
    painter.rect_filled(viewport, 0., INK);
    let result = paint_model_parts(&painter, f, id, viewport);
    if let Err(error) = result {
        painter.text(
            viewport.center(),
            egui::Align2::CENTER_CENTER,
            error,
            egui::FontId::monospace(16.),
            egui::Color32::LIGHT_RED,
        );
    }
}

fn paint_model_parts(
    painter: &egui::Painter,
    f: &Frontend,
    id: &str,
    viewport: egui::Rect,
) -> Result<(), String> {
    use crate::player_model::{ModelPlacement, builtin, draw_egui, resolve_model};
    let catalogue = builtin()?;
    let model = catalogue
        .model(id)
        .ok_or_else(|| format!("Unknown model {id}"))?;
    let source = catalogue
        .source(&model.definition.source.id)
        .ok_or("Missing original source")?;
    let texture = f.textures.menu_idle.ok_or("Original texture is loading")?;
    let bindings = std::collections::BTreeMap::from([(source.id.clone(), texture)]);
    let original = resolve_model(
        catalogue,
        id,
        ModelPlacement {
            source_origin: model.definition.ground,
            scale: 1.,
        },
    )?;
    let ppp = painter.ctx().pixels_per_point();
    let unit = 1. / ppp;
    painter.text(viewport.left_top() + egui::vec2(14., 12.) * unit, egui::Align2::LEFT_TOP,
        format!("{id} · original-source ownership · {} actual independently rendered parts · inspect semantic boundaries, fingers and limb identity", original.packets.len()),
        egui::FontId::monospace(14. * unit), egui::Color32::WHITE);
    let area = viewport.shrink2(egui::vec2(12., 12.) * unit);
    let area = egui::Rect::from_min_max(
        area.left_top() + egui::vec2(0., 32.) * unit,
        area.right_bottom(),
    );
    let columns = 5;
    let rows = original.packets.len().div_ceil(columns);
    let cell_size = egui::vec2(area.width() / columns as f32, area.height() / rows as f32);
    for (index, packet) in original.packets.iter().enumerate() {
        let cell = egui::Rect::from_min_size(
            area.left_top()
                + egui::vec2(
                    (index % columns) as f32 * cell_size.x,
                    (index / columns) as f32 * cell_size.y,
                ),
            cell_size,
        )
        .shrink(3. * unit);
        let image_area = egui::Rect::from_min_max(
            cell.left_top() + egui::vec2(8., 36.) * unit,
            cell.right_bottom() - egui::vec2(8., 8.) * unit,
        );
        painter.rect_filled(cell, 0., egui::Color32::from_rgb(20, 28, 26));
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(image_area.center().x, image_area.top()),
                image_area.right_bottom(),
            ),
            0.,
            egui::Color32::from_rgb(226, 226, 218),
        );
        painter.text(
            cell.left_top() + egui::vec2(5., 4.) * unit,
            egui::Align2::LEFT_TOP,
            &packet.part_id,
            egui::FontId::monospace(13. * unit),
            egui::Color32::WHITE,
        );
        painter.text(
            cell.left_top() + egui::vec2(5., 20.) * unit,
            egui::Align2::LEFT_TOP,
            format!("{} · {} rectangles", packet.slot, packet.rectangles.len()),
            egui::FontId::monospace(11. * unit),
            egui::Color32::GRAY,
        );
        if packet.rectangles.is_empty() {
            painter.text(
                image_area.center(),
                egui::Align2::CENTER_CENTER,
                "No visible source pixels\nOccluded at bind",
                egui::FontId::monospace(12. * unit),
                egui::Color32::YELLOW,
            );
            continue;
        }
        let left = packet
            .rectangles
            .iter()
            .map(|rect| rect.x0)
            .min()
            .ok_or("Empty visible part")?;
        let top = packet.rectangles.iter().map(|rect| rect.y0).min().unwrap();
        let right = packet.rectangles.iter().map(|rect| rect.x1).max().unwrap();
        let bottom = packet.rectangles.iter().map(|rect| rect.y1).max().unwrap();
        let scale = (image_area.width() / (right - left) as f32)
            .min(image_area.height() / (bottom - top) as f32);
        let position = image_area.center()
            - egui::vec2((right - left) as f32, (bottom - top) as f32) * (scale * 0.5);
        let mut solo = resolve_model(
            catalogue,
            id,
            ModelPlacement {
                source_origin: [
                    position.x + (model.definition.ground[0] - left as f32) * scale,
                    position.y + (model.definition.ground[1] - top as f32) * scale,
                ],
                scale,
            },
        )?;
        solo.packets.retain(|part| part.part_id == packet.part_id);
        draw_egui(&painter.with_clip_rect(image_area), &solo, &bindings)?;
    }
    Ok(())
}

#[derive(Clone, serde::Serialize)]
pub(crate) struct NativeMatrixReport {
    schema_version: u32,
    kind: &'static str,
    model_id: String,
    cases: Vec<NativeMatrixCaseReport>,
}

#[derive(Clone, serde::Serialize)]
struct NativeMatrixCaseReport {
    label: String,
    error: Option<String>,
    packet_count: usize,
    vertices: usize,
    indices: usize,
    part_ids: Vec<String>,
    source_ids: Vec<String>,
    near_contact_error: Option<f32>,
    far_contact_error: Option<f32>,
}

fn failed_matrix_case(
    painter: &egui::Painter,
    image: egui::Rect,
    label: String,
    error: String,
) -> NativeMatrixCaseReport {
    let font = egui::FontId::monospace(11. / painter.ctx().pixels_per_point());
    let galley = painter.layout(
        format!("UNSUPPORTED\n{error}"),
        font,
        egui::Color32::LIGHT_RED,
        image.width(),
    );
    painter
        .with_clip_rect(image)
        .galley(image.left_top(), galley, egui::Color32::LIGHT_RED);
    NativeMatrixCaseReport {
        label,
        error: Some(error),
        packet_count: 0,
        vertices: 0,
        indices: 0,
        part_ids: Vec::new(),
        source_ids: Vec::new(),
        near_contact_error: None,
        far_contact_error: None,
    }
}

fn matrix_report_id() -> egui::Id {
    egui::Id::new("lastlight.player_model.matrix_report.v1")
}

pub(super) fn matrix_report(ctx: &egui::Context) -> Option<NativeMatrixReport> {
    ctx.data(|data| data.get_temp(matrix_report_id()))
}

#[derive(Clone, serde::Serialize)]
#[serde(untagged)]
pub(crate) enum NativeModelReport {
    Parity(Box<NativeParityLayout>),
    Matrix(NativeMatrixReport),
}

pub(super) fn model_report(ctx: &egui::Context) -> Option<NativeModelReport> {
    parity_layout(ctx)
        .map(|layout| NativeModelReport::Parity(Box::new(layout)))
        .or_else(|| matrix_report(ctx).map(NativeModelReport::Matrix))
}

struct ModelMatrixCase {
    label: String,
    equipment: Option<horde_core::survivors::EquipmentVisual>,
    sample: crate::player_model::PlayerPoseSample,
    direction: [f32; 2],
}

fn model_matrix_cases() -> Result<Vec<ModelMatrixCase>, String> {
    use crate::player_model::{PlayerAction, PlayerPoseSample, verification::review_equipment};
    use horde_core::weapons::WeaponClass;
    let default_sample = PlayerPoseSample {
        tick: 180,
        aim: [0.; 2],
        action: PlayerAction::Hold,
        recoil: 0.,
        brace: 0.,
        hit: 0.,
        reduced_effects: false,
    };
    let mut cases = vec![ModelMatrixCase {
        label: "Original source bind".into(),
        equipment: None,
        sample: default_sample,
        direction: [1., 0.],
    }];
    let mut add =
        |label: &str, class, outfit, accessory, action, direction| -> Result<(), String> {
            cases.push(ModelMatrixCase {
                label: label.into(),
                equipment: Some(review_equipment(class, outfit, accessory)?),
                sample: PlayerPoseSample {
                    action,
                    ..default_sample
                },
                direction,
            });
            Ok(())
        };
    add(
        "Selected real starter / bind",
        Some(WeaponClass::Pistol),
        Some("armor.cloth"),
        Some("accessory.pouch"),
        PlayerAction::Bind,
        [1., 0.],
    )?;
    add(
        "No accessory",
        Some(WeaponClass::Pistol),
        Some("armor.cloth"),
        None,
        PlayerAction::Hold,
        [1., 0.],
    )?;
    add(
        "No owned outfit / base garment",
        Some(WeaponClass::Pistol),
        None,
        Some("accessory.pouch"),
        PlayerAction::Hold,
        [1., 0.],
    )?;
    add(
        "All slots absent",
        None,
        None,
        None,
        PlayerAction::Hold,
        [1., 0.],
    )?;
    for style in ["armor.leather", "armor.vest", "armor.plate"] {
        add(
            style,
            Some(WeaponClass::Pistol),
            Some(style),
            None,
            PlayerAction::Hold,
            [1., 0.],
        )?;
    }
    for class in WeaponClass::ALL {
        add(
            class.name(),
            Some(class),
            Some("armor.cloth"),
            Some("accessory.pouch"),
            PlayerAction::Hold,
            [1., 0.],
        )?;
    }
    for class in [WeaponClass::Pistol, WeaponClass::Rifle] {
        for (name, direction) in [("up", [0., -1.]), ("down", [0., 1.]), ("left", [-1., 0.])] {
            add(
                &format!("{} / {name}", class.name()),
                Some(class),
                Some("armor.cloth"),
                Some("accessory.pouch"),
                PlayerAction::Hold,
                direction,
            )?;
        }
        for (name, action) in [
            ("reload .25", PlayerAction::Reload { phase: 0.25 }),
            ("reload .65", PlayerAction::Reload { phase: 0.65 }),
            ("throw .00 / release", PlayerAction::Throw { phase: 0. }),
            ("throw .60", PlayerAction::Throw { phase: 0.6 }),
            ("melee .50", PlayerAction::Melee { phase: 0.5 }),
        ] {
            add(
                &format!("{} / {name}", class.name()),
                Some(class),
                Some("armor.cloth"),
                Some("accessory.pouch"),
                action,
                [1., -0.15],
            )?;
        }
    }
    let starter = review_equipment(
        Some(WeaponClass::Pistol),
        Some("armor.cloth"),
        Some("accessory.pouch"),
    )?;
    for (label, recoil, brace, hit, reduced_effects) in [
        ("Firing / recoil", 1., 0., 0., false),
        ("Braced", 0., 1., 0., false),
        ("Hit reaction", 0., 0., 1., false),
        ("Reduced effects / hold", 0., 0., 0., true),
    ] {
        cases.push(ModelMatrixCase {
            label: label.into(),
            equipment: Some(starter.clone()),
            sample: PlayerPoseSample {
                recoil,
                brace,
                hit,
                reduced_effects,
                ..default_sample
            },
            direction: [1., 0.],
        });
    }
    Ok(cases)
}

fn model_matrix(root: &egui::Ui, id: &str) {
    assert!(
        !crate::environment::flag("LASTLIGHT_PRACTICE"),
        "Model matrix is isolated review only"
    );
    assert!(
        crate::environment::path("LASTLIGHT_REVIEW_CAPTURE").is_some(),
        "Model matrix requires explicit native capture"
    );
    let ctx = root.ctx();
    ctx.data_mut(|data| data.remove::<NativeMatrixReport>(matrix_report_id()));
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("lastlight_model_matrix"),
    ));
    painter.rect_filled(ctx.content_rect(), 0., INK);
    let result = paint_model_matrix(&painter, id, ctx.content_rect());
    match result {
        Ok(report) => {
            ctx.data_mut(|data| data.insert_temp(matrix_report_id(), report));
        }
        Err(error) => {
            painter.text(
                ctx.content_rect().center(),
                egui::Align2::CENTER_CENTER,
                error,
                egui::FontId::monospace(16.),
                egui::Color32::LIGHT_RED,
            );
        }
    }
}

fn paint_model_matrix(
    painter: &egui::Painter,
    id: &str,
    viewport: egui::Rect,
) -> Result<NativeMatrixReport, String> {
    use crate::player_model::{ModelPlacement, draw_egui, resolve_equipped_model, resolve_model};
    let catalogue = crate::player_model::startup::active_or_bundled()?;
    let textures = crate::player_model::backend_egui::textures(painter.ctx())?;
    let cases = model_matrix_cases()?;
    let unit = 1. / painter.ctx().pixels_per_point();
    painter.text(viewport.left_top() + egui::vec2(12., 12.) * unit, egui::Align2::LEFT_TOP,
        format!("{id} · actual production appearance / pose / texture pipeline · source bind retained · inspect grip, seams, two arms and distinct feet"), egui::FontId::monospace(14. * unit), egui::Color32::WHITE);
    let area = egui::Rect::from_min_max(
        viewport.left_top() + egui::vec2(12., 44.) * unit,
        viewport.right_bottom() - egui::vec2(12., 12.) * unit,
    );
    let columns = 6;
    let rows = cases.len().div_ceil(columns);
    let cell_size = egui::vec2(area.width() / columns as f32, area.height() / rows as f32);
    let mut reports = Vec::with_capacity(cases.len());
    for (index, case) in cases.into_iter().enumerate() {
        let cell = egui::Rect::from_min_size(
            area.left_top()
                + egui::vec2(
                    (index % columns) as f32 * cell_size.x,
                    (index / columns) as f32 * cell_size.y,
                ),
            cell_size,
        )
        .shrink(3. * unit);
        painter.rect_filled(cell, 0., egui::Color32::from_rgb(24, 34, 30));
        painter.text(
            cell.left_top() + egui::vec2(6., 4.) * unit,
            egui::Align2::LEFT_TOP,
            &case.label,
            egui::FontId::monospace(13. * unit),
            egui::Color32::WHITE,
        );
        let image = egui::Rect::from_min_max(
            cell.left_top() + egui::vec2(10., 36.) * unit,
            cell.right_bottom() - egui::vec2(10., 8.) * unit,
        );
        let resolve = |placement: ModelPlacement| {
            if let Some(equipment) = &case.equipment {
                let sample = crate::player_model::PlayerPoseSample {
                    aim: [
                        placement.source_origin[0] + case.direction[0] * 10000. * placement.scale,
                        placement.source_origin[1] + case.direction[1] * 10000. * placement.scale,
                    ],
                    ..case.sample
                };
                resolve_equipped_model(catalogue, id, equipment, sample, placement)
            } else {
                resolve_model(catalogue, id, placement)
            }
        };
        let unplaced = match resolve(ModelPlacement {
            source_origin: [0.; 2],
            scale: 1.,
        }) {
            Ok(pose) => pose,
            Err(error) => {
                reports.push(failed_matrix_case(painter, image, case.label, error));
                continue;
            }
        };
        let mut bounds = egui::Rect::NOTHING;
        for packet in unplaced
            .packets
            .iter()
            .filter(|packet| packet.channel != crate::player_model::SourceChannel::Residue)
        {
            for vertex in &packet.local_mesh.vertices {
                let p = crate::player_model::verification::packet_point(packet, vertex.position);
                bounds.extend_with(egui::pos2(p[0], p[1]));
            }
        }
        if !bounds.is_finite() || bounds.width() <= 0. || bounds.height() <= 0. {
            reports.push(failed_matrix_case(
                painter,
                image,
                case.label,
                format!("Invalid actual model geometry: {id}"),
            ));
            continue;
        }
        let scale = (image.width() / bounds.width()).min(image.height() / bounds.height());
        let origin = [
            image.center().x - bounds.center().x * scale,
            image.bottom() - bounds.bottom() * scale,
        ];
        let pose = match resolve(ModelPlacement {
            source_origin: origin,
            scale,
        }) {
            Ok(pose) => pose,
            Err(error) => {
                reports.push(failed_matrix_case(painter, image, case.label, error));
                continue;
            }
        };
        if let Err(error) = draw_egui(&painter.with_clip_rect(image), &pose, &textures) {
            reports.push(failed_matrix_case(painter, image, case.label, error));
            continue;
        }
        let error = |contact: &str, target: &str| {
            pose.sockets
                .get(contact)
                .zip(pose.sockets.get(target))
                .map(|(a, b)| (a[0] - b[0]).hypot(a[1] - b[1]))
        };
        let near_contact_error = error("contact.near", "grip");
        let far_contact_error = error("contact.far", "support");
        painter.text(
            cell.left_top() + egui::vec2(6., 20.) * unit,
            egui::Align2::LEFT_TOP,
            format!(
                "{} parts · contact {:.3?} / {:.3?}",
                pose.packets.len(),
                near_contact_error,
                far_contact_error
            ),
            egui::FontId::monospace(10. * unit),
            egui::Color32::GRAY,
        );
        reports.push(NativeMatrixCaseReport {
            label: case.label,
            error: None,
            packet_count: pose.packets.len(),
            vertices: pose
                .packets
                .iter()
                .map(|packet| packet.local_mesh.vertices.len())
                .sum(),
            indices: pose
                .packets
                .iter()
                .map(|packet| packet.local_mesh.indices.len())
                .sum(),
            part_ids: pose
                .packets
                .iter()
                .map(|packet| packet.part_id.clone())
                .collect(),
            source_ids: pose
                .packets
                .iter()
                .map(|packet| packet.source_id.clone())
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect(),
            near_contact_error,
            far_contact_error,
        });
    }
    Ok(NativeMatrixReport {
        schema_version: 1,
        kind: "player_model_matrix",
        model_id: id.into(),
        cases: reports,
    })
}

/// Apply only a common review-camera projection to an already resolved pose.
/// Bone/source transforms, cached geometry and selection remain unchanged.
fn project_review_pose(
    pose: &crate::player_model::ResolvedPlayerPose,
    origin: [f32; 2],
    scale: f32,
) -> crate::player_model::ResolvedPlayerPose {
    let project = |p: [f32; 2]| [origin[0] + p[0] * scale, origin[1] + p[1] * scale];
    let mut out = pose.clone();
    for packet in &mut out.packets {
        packet.transform.source_origin = project(packet.transform.source_origin);
        packet.transform.scale *= scale;
    }
    for point in out.sockets.values_mut() {
        *point = project(*point);
    }
    out.muzzle = project(out.muzzle);
    out.throw_origin = project(out.throw_origin);
    out
}

fn model_repairs(root: &egui::Ui, id: &str) {
    assert!(
        !crate::environment::flag("LASTLIGHT_PRACTICE"),
        "Repair isolation is diagnostic only"
    );
    assert!(
        crate::environment::path("LASTLIGHT_REVIEW_CAPTURE").is_some(),
        "Repair isolation requires explicit native capture"
    );
    let ctx = root.ctx();
    ctx.data_mut(|data| data.remove::<NativeMatrixReport>(matrix_report_id()));
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("lastlight_model_repairs"),
    ));
    painter.rect_filled(ctx.content_rect(), 0., INK);
    match paint_model_repairs(&painter, id, ctx.content_rect()) {
        Ok(report) => {
            ctx.data_mut(|data| data.insert_temp(matrix_report_id(), report));
        }
        Err(error) => {
            painter.text(
                ctx.content_rect().center(),
                egui::Align2::CENTER_CENTER,
                error,
                egui::FontId::monospace(16.),
                egui::Color32::LIGHT_RED,
            );
        }
    }
}

fn paint_model_repairs(
    painter: &egui::Painter,
    id: &str,
    viewport: egui::Rect,
) -> Result<NativeMatrixReport, String> {
    use crate::player_model::{
        ModelPlacement, PlayerAction, PlayerPoseSample, SourceChannel,
        backend_egui::textures,
        draw_egui, resolve_equipped_model, resolve_model,
        startup::active_or_bundled,
        verification::{packet_point, review_equipment},
    };
    use horde_core::weapons::WeaponClass;
    let catalogue = active_or_bundled()?;
    let bindings = textures(painter.ctx())?;
    let unit = 1. / painter.ctx().pixels_per_point();
    painter.text(viewport.left_top() + egui::vec2(12., 12.) * unit, egui::Align2::LEFT_TOP,
        format!("{id} · causal repair isolation · each row uses ONE resolved held pose, only HiddenFill packet filtering and a common camera translation · holes are intentional"), egui::FontId::monospace(14. * unit), egui::Color32::WHITE);
    let area = egui::Rect::from_min_max(
        viewport.left_top() + egui::vec2(12., 44.) * unit,
        viewport.right_bottom() - egui::vec2(12., 12.) * unit,
    );
    let cell_size = egui::vec2(area.width() / 4., area.height() / 2.);
    let mut reports = Vec::with_capacity(8);
    for (row, class) in [WeaponClass::Pistol, WeaponClass::Rifle]
        .into_iter()
        .enumerate()
    {
        let equipment =
            review_equipment(Some(class), Some("armor.cloth"), Some("accessory.pouch"))?;
        // Identical to the matrix's right-facing held case, in unit display
        // coordinates. Resolve once; never rerun the rig for filtered columns.
        let pose = resolve_equipped_model(
            catalogue,
            id,
            &equipment,
            PlayerPoseSample {
                tick: 180,
                aim: [10000., 0.],
                action: PlayerAction::Hold,
                recoil: 0.,
                brace: 0.,
                hit: 0.,
                reduced_effects: false,
            },
            ModelPlacement {
                source_origin: [0.; 2],
                scale: 1.,
            },
        )?;
        let reference = resolve_model(
            catalogue,
            id,
            ModelPlacement {
                source_origin: [0.; 2],
                scale: 1.,
            },
        )?;
        let mut bounds = egui::Rect::NOTHING;
        for candidate in [&pose, &reference] {
            for packet in candidate
                .packets
                .iter()
                .filter(|packet| packet.channel != SourceChannel::Residue)
            {
                for vertex in &packet.local_mesh.vertices {
                    let point = packet_point(packet, vertex.position);
                    bounds.extend_with(egui::pos2(point[0], point[1]));
                }
            }
        }
        if !bounds.is_finite() || bounds.width() <= 0. || bounds.height() <= 0. {
            return Err("Invalid repair-review bounds".into());
        }
        let image_height = cell_size.y - 54. * unit;
        let scale =
            ((cell_size.x - 30. * unit) / bounds.width()).min(image_height / bounds.height());
        for (column, caption) in [
            "Full actual held pose",
            "Same pose / no HiddenFill",
            "Same pose / HiddenFill only",
            "Original art / pistol bind / same height",
        ]
        .into_iter()
        .enumerate()
        {
            let cell = egui::Rect::from_min_size(
                area.left_top() + egui::vec2(column as f32 * cell_size.x, row as f32 * cell_size.y),
                cell_size,
            )
            .shrink(3. * unit);
            let image = egui::Rect::from_min_max(
                cell.left_top() + egui::vec2(12., 40.) * unit,
                cell.right_bottom() - egui::vec2(12., 10.) * unit,
            );
            painter.rect_filled(cell, 0., egui::Color32::from_rgb(24, 34, 30));
            let mut selected = if column == 3 {
                reference.clone()
            } else {
                pose.clone()
            };
            if column == 1 {
                selected
                    .packets
                    .retain(|packet| packet.channel != SourceChannel::HiddenFill);
            }
            if column == 2 {
                selected
                    .packets
                    .retain(|packet| packet.channel == SourceChannel::HiddenFill);
            }
            let origin = [
                image.center().x - bounds.center().x * scale,
                image.bottom() - bounds.bottom() * scale,
            ];
            let projected = project_review_pose(&selected, origin, scale);
            draw_egui(&painter.with_clip_rect(image), &projected, &bindings)?;
            let label = format!("{} / {caption}", class.name());
            painter.text(
                cell.left_top() + egui::vec2(6., 4.) * unit,
                egui::Align2::LEFT_TOP,
                &label,
                egui::FontId::monospace(14. * unit),
                egui::Color32::WHITE,
            );
            painter.text(
                cell.left_top() + egui::vec2(6., 23.) * unit,
                egui::Align2::LEFT_TOP,
                format!(
                    "{} packets · {} HiddenFill · identical root scale {:.4}",
                    selected.packets.len(),
                    selected
                        .packets
                        .iter()
                        .filter(|packet| packet.channel == SourceChannel::HiddenFill)
                        .count(),
                    scale
                ),
                egui::FontId::monospace(11. * unit),
                egui::Color32::GRAY,
            );
            reports.push(NativeMatrixCaseReport {
                label,
                error: None,
                packet_count: selected.packets.len(),
                vertices: selected
                    .packets
                    .iter()
                    .map(|packet| packet.local_mesh.vertices.len())
                    .sum(),
                indices: selected
                    .packets
                    .iter()
                    .map(|packet| packet.local_mesh.indices.len())
                    .sum(),
                part_ids: selected
                    .packets
                    .iter()
                    .map(|packet| packet.part_id.clone())
                    .collect(),
                source_ids: selected
                    .packets
                    .iter()
                    .map(|packet| packet.source_id.clone())
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter()
                    .collect(),
                near_contact_error: None,
                far_contact_error: None,
            });
        }
    }
    Ok(NativeMatrixReport {
        schema_version: 1,
        kind: "player_model_repairs",
        model_id: id.into(),
        cases: reports,
    })
}

pub(super) fn prepare(f: &mut Frontend) {
    let Some(screen) = crate::environment::string("LASTLIGHT_REVIEW_SCREEN") else {
        return;
    };
    if screen == "npc-stutter" {
        prepare_copied_npc(f);
        return;
    }
    f.game = Game::default();
    if !crate::environment::flag("LASTLIGHT_PRACTICE") {
        f.settings.master = 0.0;
    }
    if let Ok(scale) = std::env::var("LASTLIGHT_REVIEW_SCALE") {
        f.settings.scale = scale.parse().unwrap_or(1.0);
    }
    f.message = "Practice scenario - progress is isolated from your campaign".into();
    if screen == "menu" {
        return;
    }
    if screen == "survivor-select" {
        f.screen = Screen::SurvivorSelection;
        f.preview_survivor = SurvivorId::Rook;
        return;
    }
    let identity = if matches!(
        screen.as_str(),
        "menu-female" | "battle-female" | "gesture-melee"
    ) {
        SurvivorId::June
    } else {
        SurvivorId::Rook
    };
    f.game
        .campaign
        .choose_survivor(identity)
        .expect("review survivor identity");
    f.preview_survivor = identity;
    if screen == "menu-female" {
        f.screen = Screen::Menu;
        return;
    }
    if matches!(screen.as_str(), "peak" | "progressed-peak" | "max-lanes") {
        f.game = if matches!(screen.as_str(), "progressed-peak" | "max-lanes") {
            horde_core::sim::maximum_progression_render_fixture()
                .expect("maximum authored progression render fixture")
        } else {
            horde_core::sim::peak_render_fixture().expect("valid paused peak-render fixture")
        };
        f.screen = Screen::Battle;
        f.mode = Mode::Npc;
        f.selected_country = "USA".into();
        f.message = if matches!(screen.as_str(),"progressed-peak"|"max-lanes") { "Synthetic maximum-rank fixture · authored population cap plus boss · no gameplay or FPS claim" } else { "Paused peak-render fixture · no simulation FPS claim" }.into();
        return;
    }
    f.screen = Screen::Camp;
    if screen == "history-empty" {
        f.screen = Screen::History;
        f.history_return = Screen::Menu;
        f.history_world_paused = true;
        f.game.set_paused(true);
        return;
    }
    if screen == "world" {
        f.game.set_paused(true);
        return;
    }
    let country = match screen.as_str() {
        "arid" => "EGY",
        "tropical" => "BRA",
        _ => "DEU",
    };
    f.game
        .campaign
        .choose_start_country(country)
        .expect("review country");
    f.selected_country = country.into();
    if matches!(screen.as_str(), "briefing" | "briefing-npc") {
        if screen == "briefing-npc" {
            prepare_progression(&mut f.game);
            f.mode = Mode::Npc;
        }
        f.open_briefing();
        f.game.set_paused(true);
        return;
    }
    if matches!(screen.as_str(), "journey-victory" | "journey-liberated") {
        prepare_journey_victory(f, screen == "journey-liberated");
        return;
    }
    if screen == "journey-upgrades" {
        f.game
            .deploy_country_stage(0, Mode::Personal)
            .expect("first journey review round");
        f.game.set_paused(false);
        for _ in 0..60 * 300 {
            if f.game.attempt.is_none() {
                break;
            }
            let input = horde_core::sim::smoke_input(&f.game);
            f.game.tick(input);
        }
        if f.game.attempt.is_some() {
            f.game.withdraw();
        }
        f.game.set_paused(true);
        f.mode = Mode::Personal;
        f.focus_next_upgrade();
        f.screen = Screen::Results;
        return;
    }
    if screen == "intro" {
        f.game
            .deploy_country_stage(0, Mode::Personal)
            .expect("first-defense fixture");
        f.screen = Screen::Battle;
        f.mode = Mode::Personal;
        f.game.set_paused(true);
        return;
    }
    if matches!(screen.as_str(), "progressed-battle" | "leveled-armory") {
        let count = f
            .game
            .campaign
            .world
            .active_country()
            .expect("review country")
            .level_count();
        // Explicit completed-run entitlements for a staged view, not an earned
        // balance proof. Repeated templates still count as victories.
        for stage in 0..count {
            f.game
                .campaign
                .first_clear(stage % content().levels.len(), Mode::Personal);
            f.game.campaign.finish_attempt();
            f.game
                .campaign
                .record_country_stage_clear(stage)
                .expect("explicit review progression entitlement");
        }
        if screen == "progressed-battle" {
            f.game
                .campaign
                .enter_country("POL")
                .expect("neighbor after liberation");
            f.selected_country = "POL".into();
        }
    }
    prepare_progression(&mut f.game);
    match screen.as_str() {
        "gesture-throws" | "gesture-melee" => {
            assert!(
                gesture_fixture(
                    Some(&screen),
                    crate::environment::flag("LASTLIGHT_REVIEW_MOTION"),
                    crate::environment::path("LASTLIGHT_CAPTURE_SEQUENCE").is_some(),
                    crate::environment::flag("LASTLIGHT_PRACTICE")
                )
                .is_some(),
                "Gesture fixtures require isolated motion+sequence opt-ins"
            );
            prepare_gesture_battle(f, screen == "gesture-melee");
        }
        "character" => f.tab = Tab::Character,
        "character-max" => {
            f.game.campaign.player.level = f.game.campaign.player_level_cap();
            f.game.campaign.player.xp = 0;
            f.game.campaign.player.points =
                f.game.campaign.player.level - 1 - f.game.campaign.player.body.iter().sum::<u32>();
            f.tab = Tab::Character;
        }
        "campaign" => f.tab = Tab::World,
        "active-camp" => {
            prepare_battle(f, "npc");
            f.screen = Screen::Camp;
            f.tab = Tab::World;
        }
        "survivor" => f.tab = Tab::Survivor,
        "armory" | "leveled-armory" | "quality-armory" => {
            f.tab = Tab::Armory;
            if screen == "quality-armory" {
                let definition = content()
                    .items
                    .iter()
                    .position(|i| i.slot == Slot::Ranged && i.tier == 1)
                    .expect("review rifle");
                for quality in horde_core::quality::Quality::ALL {
                    f.selected_item = Some(
                        f.game
                            .campaign
                            .add_item_with_quality(definition, 4, Some(quality))
                            .expect("explicit quality fixture entitlement"),
                    );
                }
                f.armory_sort = armory::ArmorySort::Quality;
                f.game.set_paused(true);
                return;
            }
            f.selected_item = f
                .game
                .campaign
                .items
                .iter()
                .find(|i| !i.protected)
                .map(|i| i.id);
        }
        "skills" | "skills-survival" | "skills-ordnance" => {
            f.screen = Screen::Results;
            f.tab = Tab::Skills;
            f.improvement_focus = Some(horde_core::campaign::RecommendationAction::LearnNode(
                match screen.as_str() {
                    "skills-survival" => "field_fitness",
                    "skills-ordnance" => "grenade_training",
                    _ => "pistol_training",
                }
                .into(),
            ));
        }
        "engineering" => {
            f.screen = Screen::Results;
            f.improvement_focus = horde_core::skill_catalog::skills()
                .nodes
                .iter()
                .find(|n| n.branch == "Fortification")
                .map(|n| horde_core::campaign::RecommendationAction::LearnNode(n.id.clone()));
        }
        "history" => prepare_history(f),
        "settings" => f.screen = Screen::Settings,
        "operations" => prepare_operations(f),
        "reticle-reload" | "damage" => {
            prepare_battle(f, "battle");
            f.game.set_paused(false);
            if screen == "reticle-reload" {
                // Consume actual shots; the resource readout never invents ammo.
                let half = f.game.attempt.as_ref().unwrap().magazine / 2;
                for _ in 0..240 {
                    let a = f.game.attempt.as_ref().expect("live reload review");
                    if a.ammo <= half {
                        break;
                    }
                    let aim = [a.width_yards * 0.6, a.defender_position()[1]];
                    f.game.tick(Input {
                        fire: true,
                        aim,
                        auto_reload: false,
                        ..Default::default()
                    });
                }
                f.game.tick(Input {
                    reload: true,
                    ..Default::default()
                });
            } else {
                let target = f
                    .game
                    .attempt
                    .as_ref()
                    .unwrap()
                    .enemies
                    .first()
                    .map(|e| [e.x, e.y()])
                    .unwrap_or([
                        f.game.attempt.as_ref().unwrap().width_yards * 0.6,
                        f.game.attempt.as_ref().unwrap().defender_position()[1],
                    ]);
                f.game.tick(Input {
                    aim: target,
                    fire: true,
                    ..Default::default()
                });
            }
            f.aim = [
                f.game.attempt.as_ref().unwrap().width_yards * 0.6,
                f.game.attempt.as_ref().unwrap().defender_position()[1],
            ];
        }
        "damage-4x" => {
            prepare_battle(f, "npc");
            f.speed = 4;
            f.aim = [
                f.game.attempt.as_ref().unwrap().width_yards * 0.6,
                f.game.attempt.as_ref().unwrap().defender_position()[1],
            ];
        }
        "npc-expanded" => prepare_battle(f, "npc"),
        "critical-npc" => {
            prepare_battle(f, "npc");
            let mut a = f.game.attempt.take().expect("live NPC review");
            let lane = a.lane_count / 2;
            f.game
                .spawn_enemy(&mut a, horde_core::content::EnemyKind::Brute, lane, false);
            let enemy = a.enemies.last_mut().unwrap();
            enemy.x = 9.0;
            enemy.hp = -2;
            enemy.injury.hp = -2;
            f.game.attempt = Some(a);
            f.speed = 4;
        }
        "quality-loot" => {
            prepare_battle(f, "battle");
            let definition = content()
                .items
                .iter()
                .position(|i| i.slot == Slot::Ranged && i.tier == 1)
                .expect("review rifle");
            let quality = horde_core::quality::Quality::Epic;
            let item_id = f
                .game
                .campaign
                .add_item_with_quality(definition, 3, Some(quality))
                .expect("explicit isolated loot fixture");
            let a = f.game.attempt.as_mut().unwrap();
            a.loot += 1;
            a.loot_events.push(horde_core::quality::LootEvent {
                item_id,
                definition,
                level: 3,
                quality,
                name: content().items[definition].name.clone(),
                tick: a.tick,
                at: [a.width_yards * 0.62, a.defender_position()[1]],
            });
        }
        "battle" | "battle-female" | "progressed-battle" | "construction" | "ordnance" | "npc"
        | "boss" | "results" | "arid" | "tropical" => prepare_battle(f, &screen),
        _ => f.tab = Tab::World,
    }
    f.game.set_paused(true);
}
/// Strict opt-in copied checkpoint; production and source files are never targets.
fn prepare_copied_npc(f: &mut Frontend) {
    assert!(
        !crate::environment::flag("LASTLIGHT_PRACTICE"),
        "npc-stutter requires isolated non-practice review"
    );
    let input = crate::environment::path("LASTLIGHT_REVIEW_INPUT_SAVE")
        .expect("npc-stutter requires LASTLIGHT_REVIEW_INPUT_SAVE copied generation");
    let canonical = input
        .canonicalize()
        .expect("copied NPC checkpoint must exist");
    let copied_settings = input
        .parent()
        .expect("checkpoint parent")
        .join("settings.json");
    for source in [&canonical, &copied_settings] {
        if let Ok(source) = source.canonicalize() {
            for target in [
                &f.save_path,
                &f.save_path.with_extension("bak"),
                &f.save_path.with_extension("tmp"),
                &f.settings_path,
            ] {
                if let Ok(target) = target.canonicalize() {
                    assert_ne!(
                        source, target,
                        "Copied inputs must differ from every review output generation"
                    );
                }
            }
        }
    }
    let bytes = std::fs::read(&canonical).expect("read copied NPC checkpoint");
    f.game = Game::decode_save_generation(&bytes)
        .expect("Copied NPC generation invalid; no fallback is permitted");
    assert!(
        f.game.attempt.as_ref().is_some_and(|a| a.mode == Mode::Npc),
        "Copied checkpoint must contain the active NPC battle"
    );
    f.settings = Settings::load(&copied_settings);
    if !(crate::environment::flag("LASTLIGHT_PERF_AUDIO")
        && crate::environment::path("LASTLIGHT_PERF_REPORT").is_some())
    {
        f.settings.master = 0.0;
    }
    if let Some(scale) =
        crate::environment::string("LASTLIGHT_REVIEW_SCALE").and_then(|s| s.parse().ok())
    {
        f.settings.scale = scale;
    }
    f.game.set_paused(false);
    f.mode = Mode::Npc;
    f.screen = Screen::Battle;
    f.speed = crate::environment::string("LASTLIGHT_PERF_NPC_SPEED")
        .map(|value| {
            value
                .parse::<u32>()
                .expect("NPC diagnostic speed must be1,2or4")
        })
        .unwrap_or(1);
    assert!(
        matches!(f.speed, 1 | 2 | 4),
        "NPC diagnostic speed must be1,2or4"
    );
    let a = f.game.attempt.as_ref().unwrap();
    f.aim = [a.width_yards * 0.6, a.defender_position()[1]];
    f.message = "Copied checkpoint/resumed NPC battle · isolated performance review".into();
}

/// Staged near-defeated boss; real core settlement supplies the report/rewards.
fn prepare_journey_victory(f: &mut Frontend, liberated: bool) {
    let count = f
        .game
        .campaign
        .world
        .active_country()
        .expect("review country")
        .level_count();
    let stage = if liberated { count - 1 } else { 0 };
    for n in 0..stage {
        f.game
            .campaign
            .record_country_stage_clear(n)
            .expect("explicit earlier-country review milestones");
    }
    f.game
        .deploy_country_stage(stage, Mode::Personal)
        .expect("journey outcome fixture");
    let mut a = f.game.attempt.take().unwrap();
    a.boss_spawned = true;
    let lane = a.lane_count / 2;
    f.game
        .spawn_enemy(&mut a, horde_core::content::EnemyKind::Brute, lane, true);
    let target = a.enemies.last_mut().unwrap();
    target.x = a.width_yards * 0.55;
    target.hp = 1;
    target.armor = 0;
    let aim = [target.x, target.y()];
    f.game.attempt = Some(a);
    f.game.set_paused(false);
    f.game.tick(Input {
        fire: true,
        aim,
        ..Default::default()
    });
    assert!(f.game.attempt.is_none());
    assert_eq!(
        f.game.last_result.as_ref().unwrap().outcome,
        Outcome::Victory
    );
    f.game.set_paused(true);
    f.mode = Mode::Personal;
    f.focus_next_upgrade();
    f.screen = Screen::Results;
}

fn prepare_progression(game: &mut Game) {
    // Explicit screenshot entitlements; use model methods for every owned item,
    // skill and point investment. This does not represent earned campaign income.
    let campaign = &mut game.campaign;
    // These pre-existing advanced view fixtures explicitly retain legacy ledgers.
    campaign.legacy_features = true;
    campaign.coins = 1800;
    for stage in 0..5 {
        campaign.first_clear(stage, Mode::Personal);
    }
    campaign.finish_attempt();
    campaign.reward(0, 300, Mode::Personal);
    for attribute in [
        BodyAttribute::Vitality,
        BodyAttribute::Strength,
        BodyAttribute::Endurance,
    ] {
        campaign
            .allocate_body(attribute)
            .expect("fixture body investment");
    }
    for skill in [Skill::QuickReload, Skill::SecondWind, Skill::Engineering] {
        campaign.purchase_skill(skill).expect("fixture skill");
    }
    campaign
        .purchase_node("rifle_training")
        .expect("fixture firearm proficiency");
    campaign
        .purchase_node("grenade_training")
        .expect("fixture grenade handling");
    campaign
        .purchase_node("molotov_training")
        .expect("fixture incendiary handling");
    for kind in StructureKind::ALL {
        campaign
            .purchase_blueprint(kind)
            .expect("fixture blueprint");
    }
    for slot in Slot::ALL {
        let definition = content()
            .items
            .iter()
            .enumerate()
            .find(|(_, item)| item.slot == slot && item.tier == 1)
            .map(|(n, _)| n)
            .expect("starter-tier shop option");
        let id = campaign
            .buy_item(definition)
            .expect("fixture shop equipment");
        campaign
            .equip(Mode::Personal, id)
            .expect("fixture personal equipment");
        campaign
            .equip(Mode::Npc, id)
            .expect("fixture alternate equipment");
    }
    campaign.record_npc_progress(20, 7200);
    for _ in 0..10 {
        campaign
            .buy_training_point()
            .expect("fixture training service");
    }
    campaign.train_npc(NpcTraining::Guns).expect("fixture guns");
    campaign
        .train_npc(NpcTraining::Melee)
        .expect("fixture melee");
    let definition = content()
        .items
        .iter()
        .position(|item| item.slot == Slot::Ranged && item.tier == 1)
        .unwrap();
    let id = campaign
        .add_item(definition)
        .expect("fixture recovered spare");
    campaign.set_favorite(id, true).expect("fixture favorite");
    campaign
        .save_preset("Prepared patrol".into(), Mode::Personal)
        .expect("fixture preset");
}
fn prepare_history(f: &mut Frontend) {
    // Real model deployments/inputs/settlements, never authored plot points.
    for mode in [Mode::Personal, Mode::Npc, Mode::Personal] {
        f.game
            .deploy_country_stage(0, mode)
            .expect("history review deployment");
        f.game.set_paused(false);
        for _ in 0..60 * 18 {
            if f.game.attempt.is_none() {
                break;
            }
            let input = if mode == Mode::Personal {
                horde_core::sim::smoke_input(&f.game)
            } else {
                Input::default()
            };
            f.game.tick(input);
        }
        if f.game.attempt.is_some() {
            f.game.withdraw();
        }
    }
    f.history_return = Screen::Camp;
    f.history_world_paused = true;
    f.screen = Screen::History;
}
fn prepare_battle(f: &mut Frontend, screen: &str) {
    let mode = if screen == "npc" {
        Mode::Npc
    } else {
        Mode::Personal
    };
    f.game
        .deploy_country_stage(0, mode)
        .expect("review deployment");
    let lanes = f.game.attempt.as_ref().unwrap().lane_count;
    let center = lanes / 2;
    // A legal mixed starting plan in the actual frozen rows.
    for (kind, lane, column) in [
        (StructureKind::Wall, center.saturating_sub(1), 0),
        (StructureKind::Wall, (center + 1).min(lanes - 1), 0),
        (StructureKind::Turret, center, 1),
        (StructureKind::Trap, center, 2),
    ] {
        f.game
            .place(kind, lane, column)
            .expect("fixture affordable placement");
    }
    f.game.set_paused(false);
    let target = match screen {
        "progressed-battle" => 60 * 15,
        "results" => 60 * 480,
        "boss" => ((f.game.attempt.as_ref().unwrap().boss_time() + 12.0) * 60.0).ceil() as u32,
        _ => 60 * 38,
    };
    for _ in 0..target {
        if f.game.attempt.is_none() {
            break;
        }
        let input = if matches!(screen, "boss" | "results") {
            horde_core::sim::smoke_input(&f.game)
        } else {
            Input::default()
        };
        f.game.tick(input);
        if !matches!(screen, "boss" | "results")
            && f.game
                .attempt
                .as_ref()
                .is_some_and(|a| a.elapsed() > 6.0 && a.hp < a.max_hp * 2 / 3)
        {
            break;
        }
        if screen == "boss" && f.game.attempt.as_ref().is_some_and(|a| a.boss_spawned) {
            for _ in 0..60 * 8 {
                if f.game.attempt.is_some() {
                    f.game.tick(Input::default());
                }
            }
            break;
        }
    }
    if screen == "ordnance" && f.game.attempt.is_some() {
        let a = f.game.attempt.as_ref().unwrap();
        let molotov_cell = [(a.width_yards * 0.6).floor() as u8, a.lane_count / 2];
        let grenade_cell = [
            (a.width_yards * 0.75).floor() as u8,
            (a.lane_count / 2 + 1).min(a.lane_count - 1),
        ];
        f.game.tick(Input {
            throw: Some(ThrowRequest {
                kind: OrdnanceKind::Molotov,
                cell: molotov_cell,
            }),
            ..Input::default()
        });
        for _ in 0..=horde_core::ordnance::catalog().molotov.flight_ticks {
            f.game.tick(Input::default());
        }
        f.game.tick(Input {
            throw: Some(ThrowRequest {
                kind: OrdnanceKind::Grenade,
                cell: grenade_cell,
            }),
            ..Input::default()
        });
        f.attack_selection = battle::AttackSelection::Molotov;
        f.aim = [
            f.game.attempt.as_ref().unwrap().width_yards * 0.6,
            f.game.attempt.as_ref().unwrap().defender_position()[1],
        ];
    }
    if screen == "results" {
        if f.game.attempt.is_some() {
            f.game.withdraw();
        }
        f.screen = Screen::Results;
    } else {
        assert!(
            f.game.attempt.is_some(),
            "review battle should still be active"
        );
        f.screen = Screen::Battle;
    }
    f.mode = mode;
    f.construction = screen == "construction";
    if let Some(enemy) = f.game.attempt.as_ref().and_then(|a| {
        a.enemies
            .iter()
            .find(|e| e.boss)
            .or_else(|| a.enemies.first())
    }) {
        f.aim = [enemy.x, enemy.y()];
    }
}

fn prepare_gesture_battle(f: &mut Frontend, melee: bool) {
    prepare_battle(f, "battle");
    f.fire_latched = false;
    f.speed = 1;
    f.pending = Input::default();
    // Remove automated damage using the actual legal salvage operation. The
    // single contact placement below is initial fixture staging, never an
    // invented action event, damage result, cooldown or later-frame teleport.
    let structures: Vec<_> = f
        .game
        .attempt
        .as_ref()
        .unwrap()
        .structures
        .iter()
        .map(|structure| structure.id)
        .collect();
    for id in structures {
        f.game
            .salvage(id)
            .expect("isolated gesture fixture salvage");
    }
    let attempt = f.game.attempt.as_mut().unwrap();
    if melee {
        let lane = attempt.lane_count / 2;
        let target = attempt
            .enemies
            .iter_mut()
            .find(|enemy| {
                !enemy.boss
                    && enemy.hp > 0
                    && enemy.kind != horde_core::content::EnemyKind::Demolisher
            })
            .expect("existing live non-explosive contact target");
        target.lane = lane;
        target.x = horde_core::sim::DEFENSE_LINE;
        target.pass_offset = 0.;
    }
    f.aim = [
        attempt.width_yards * 0.6,
        f32::from(attempt.lane_count / 2) + 0.5,
    ];
    f.mode = Mode::Personal;
    f.screen = Screen::Battle;
}
fn prepare_operations(f: &mut Frontend) {
    f.tab = Tab::Operations;
    for id in ["DEU", "FRA", "CHE", "AUT"] {
        f.game.campaign.world.cleared.push(id.into());
        f.game
            .campaign
            .world
            .country_levels
            .insert(id.into(), world().country(id).unwrap().level_count());
    }
    let first = f
        .game
        .strategy
        .recruit(&mut f.game.campaign, "DEU")
        .expect("review guard");
    let second = f
        .game
        .strategy
        .recruit(&mut f.game.campaign, "FRA")
        .expect("review expedition");
    f.game
        .strategy
        .assign_guard(&mut f.game.campaign, first, "DEU")
        .expect("review garrison");
    f.game
        .deploy_survivor(second, "POL", 0)
        .expect("review parallel battle");
    f.game.set_paused(false);
    for _ in 0..60 * 20 {
        f.game.tick(Input::default());
    }
    f.game
        .strategy
        .trigger_outbreak(&f.game.campaign, "DEU")
        .expect("review plague alert");
}

#[cfg(test)]
mod loot_capture_tests {
    use super::*;
    #[test]
    fn loot_replay_is_exclusive_to_explicit_isolated_capture_modes() {
        assert_eq!(
            select_loot_capture_mode(Some("quality-loot"), true, true, false, false),
            LootCaptureMode::Motion
        );
        assert_eq!(
            select_loot_capture_mode(Some("quality-loot"), false, false, true, false),
            LootCaptureMode::Still
        );
        for (scene, motion, sequence, still, practice) in [
            (None, true, true, false, false),
            (Some("battle"), true, true, false, false),
            (Some("quality-loot"), true, true, false, true),
            (Some("quality-loot"), true, false, false, false),
            (Some("quality-loot"), false, true, false, false),
            (Some("quality-loot"), false, false, false, false),
        ] {
            assert_eq!(
                select_loot_capture_mode(scene, motion, sequence, still, practice),
                LootCaptureMode::Disabled
            );
        }
    }
    fn prepared_frontend() -> Frontend {
        let mut f = Frontend::load();
        f.game = Game::default();
        f.game.campaign.choose_survivor(SurvivorId::Rook).unwrap();
        f.game.campaign.choose_start_country("DEU").unwrap();
        prepare_progression(&mut f.game);
        f
    }
    #[test]
    fn gesture_controls_require_the_complete_isolated_export_guard() {
        for scene in [Some("gesture-throws"), Some("gesture-melee")] {
            assert!(gesture_fixture(scene, true, true, false).is_some());
            for (motion, sequence, practice) in [
                (false, true, false),
                (true, false, false),
                (true, true, true),
                (false, false, false),
            ] {
                assert_eq!(gesture_fixture(scene, motion, sequence, practice), None);
            }
        }
        for scene in [None, Some("battle"), Some("ordnance"), Some("npc")] {
            assert_eq!(gesture_fixture(scene, true, true, false), None);
        }
    }
    #[test]
    fn scheduled_gesture_inputs_create_real_post_warmup_events_and_recover() {
        use horde_core::sim::EventKind;
        for kind in [GestureFixture::Throws, GestureFixture::Melee] {
            let mut f = prepared_frontend();
            prepare_gesture_battle(&mut f, kind == GestureFixture::Melee);
            f.game.set_paused(false);
            let start = f.game.attempt.as_ref().unwrap().tick;
            let mut observed_throws = std::collections::BTreeMap::new();
            let mut observed_melee = std::collections::BTreeMap::new();
            for frame in 1..=152 {
                let input = scheduled_gesture_input(
                    kind,
                    frame,
                    f.game
                        .attempt
                        .as_ref()
                        .expect("live isolated gesture battle"),
                )
                .unwrap_or_default();
                if frame <= 60 {
                    assert!(!input.melee && input.throw.is_none());
                }
                f.game.tick(input);
                // Combat VFX events legitimately expire. Observe real events
                // while live; never change production TTL to make a fixture pass.
                for event in &f.game.attempt.as_ref().expect("live gesture battle").events {
                    let key = (event.tick, event.actor_id, event.target_id);
                    match event.kind {
                        EventKind::Throw => {
                            observed_throws.entry(key).or_insert_with(|| event.clone());
                        }
                        EventKind::Melee => {
                            observed_melee.entry(key).or_insert_with(|| event.clone());
                        }
                        _ => {}
                    }
                }
            }
            let a = f
                .game
                .attempt
                .as_ref()
                .expect("gesture evidence keeps the real battle active");
            let throws: Vec<_> = observed_throws.values().collect();
            let melee: Vec<_> = observed_melee.values().collect();
            if kind == GestureFixture::Throws {
                assert_eq!(throws.len(), 2);
                assert!(
                    throws
                        .iter()
                        .all(|event| event.tick > start + 60 && event.actor_id.is_some())
                );
                assert_eq!(
                    a.ordnance.charges[0],
                    a.profile.modifiers.grenade_charges - 1
                );
                assert_eq!(
                    a.ordnance.charges[1],
                    a.profile.modifiers.molotov_charges - 1
                );
                assert!(a.tick - throws.last().unwrap().tick >= 24);
                assert!(melee.is_empty());
            } else {
                assert_eq!(melee.len(), 1);
                assert!(melee[0].tick > start + 60 && melee[0].target_id.is_some());
                assert!(melee[0].to[0] <= horde_core::sim::DEFENSE_LINE + 0.01);
                assert_eq!(a.melee_commit, 0);
                assert!(a.tick - melee[0].tick >= 30);
                assert!(throws.is_empty());
            }
        }
    }
    #[test]
    fn short_normal_review_fixtures_keep_live_battles_and_real_throw_cells_valid() {
        for scene in ["battle", "npc", "ordnance", "boss", "results"] {
            let mut f = prepared_frontend();
            let expected = f.game.campaign.arena_geometry();
            prepare_battle(&mut f, scene);
            if scene == "results" {
                assert!(f.game.last_result.is_some());
            } else {
                let a = f.game.attempt.as_ref().unwrap();
                assert_eq!((a.lane_count, a.width_yards), expected);
                for s in &a.structures {
                    assert!(s.lane < a.lane_count && s.x() < a.width_yards * 0.5);
                }
                if scene == "ordnance" {
                    assert!(!a.ordnance.projectiles.is_empty());
                    assert!(!a.ordnance.fire_zones.is_empty());
                    for p in &a.ordnance.projectiles {
                        assert!(horde_core::ordnance::valid_cell_for_arena(
                            p.cell,
                            a.lane_count,
                            a.width_yards
                        ));
                    }
                }
                if scene == "boss" {
                    assert!(a.enemies.iter().any(|e| e.boss));
                }
            }
        }
    }
    #[test]
    fn history_review_uses_real_settlements_and_damage_instead_of_authored_plot_points() {
        let mut f = prepared_frontend();
        prepare_history(&mut f);
        assert_eq!(f.game.campaign.run_history.len(), 3);
        assert!(
            f.game
                .campaign
                .run_history
                .iter()
                .any(|r| r.damage_by_source.total() > 0 && r.average_dps > 0.0)
        );
        assert!(f.screen == Screen::History);
        assert!(f.game.attempt.is_none());
    }
    #[test]
    fn new_journey_victory_and_liberation_fixtures_use_one_real_settlement_each() {
        for liberated in [false, true] {
            let mut f = Frontend::load();
            f.game = Game::default();
            f.game.campaign.choose_survivor(SurvivorId::Rook).unwrap();
            f.game.campaign.choose_start_country("DEU").unwrap();
            prepare_journey_victory(&mut f, liberated);
            assert_eq!(f.game.campaign.run_history.len(), 1);
            let coins = f.game.campaign.coins;
            let next = f.game.continue_journey().unwrap();
            assert_eq!(
                next,
                if liberated {
                    horde_core::sim::JourneyContinuation::CountryLiberated
                } else {
                    horde_core::sim::JourneyContinuation::Deployed
                }
            );
            assert_eq!(f.game.campaign.coins, coins);
            assert_eq!(f.game.campaign.run_history.len(), 1);
        }
    }
}
