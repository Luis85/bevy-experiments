//! Review-only player-model workshop, parity, parts, matrix and repair screens.
//! Built only with the `review` feature; drawn instead of the game screens when
//! an isolated review names one of them.
use super::*;

pub(in crate::ui) fn model_workshop(root: &mut egui::Ui, f: &Frontend) -> bool {
    let scene = f.launch.scenario.clone();
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
        model_matrix(root, &f.launch, id);
        return true;
    }
    if let Some(id) = scene
        .as_deref()
        .and_then(|scene| scene.strip_prefix("model-repairs-"))
    {
        model_repairs(root, &f.launch, id);
        return true;
    }
    if !matches!(
        scene.as_deref(),
        Some("model-workshop" | "model-comparison")
    ) {
        return false;
    }
    let launch = &f.launch;
    assert!(!launch.practice, "Model workshop is isolated review only");
    assert!(
        launch.capture_path().is_some() || launch.sequence_path().is_some(),
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
                player_model::rig_comparison(ui.painter(), f.textures, rect);
            } else {
                player_model::rig_review(ui.painter(), f.textures, rect);
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
    let launch = &f.launch;
    assert!(!launch.practice, "Model parity is isolated review only");
    assert!(
        launch.capture_path().is_some(),
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
    let launch = &f.launch;
    assert!(!launch.practice, "Model parts are isolated review only");
    assert!(
        launch.capture_path().is_some(),
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

pub(in crate::ui) fn model_report(ctx: &egui::Context) -> Option<NativeModelReport> {
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

fn model_matrix(root: &egui::Ui, launch: &crate::launch::LaunchConfig, id: &str) {
    assert!(!launch.practice, "Model matrix is isolated review only");
    assert!(
        launch.capture_path().is_some(),
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

fn model_repairs(root: &egui::Ui, launch: &crate::launch::LaunchConfig, id: &str) {
    assert!(!launch.practice, "Repair isolation is diagnostic only");
    assert!(
        launch.capture_path().is_some(),
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
