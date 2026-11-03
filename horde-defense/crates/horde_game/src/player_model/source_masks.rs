//! Compile authored ownership once. Source bytes are retained without changing
//! alpha, color or transparent RGB; rendering never scans an image.
use super::catalogue::*;
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Copy, Debug)]
pub struct SourceImageView<'a> {
    pub id: &'a str,
    pub width: u32,
    pub height: u32,
    pub rgba: &'a [u8],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MaskRun {
    pub y: u32,
    pub x0: u32,
    pub x1: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MaskRect {
    pub x0: u32,
    pub y0: u32,
    pub x1: u32,
    pub y1: u32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourceVertex {
    pub position: [f32; 2],
    pub uv: [f32; 2],
}
#[derive(Clone, Debug)]
pub struct LocalMesh {
    pub vertices: Vec<SourceVertex>,
    pub indices: Vec<u32>,
}
#[derive(Clone, Debug)]
pub struct CompiledSource {
    pub id: String,
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<[u8]>,
    pub rgba_fnv1a64: String,
    pub png_sha256: String,
}
impl CompiledSource {
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let i = (y as usize * self.width as usize + x as usize) * 4;
        Some(
            self.rgba[i..i + 4]
                .try_into()
                .expect("Validated RGBA source"),
        )
    }
}
#[derive(Clone, Debug)]
pub struct CompiledPart {
    pub definition: PartDefinition,
    #[cfg(test)]
    pub runs: Vec<MaskRun>,
    // Test inspection retains the pre-Arc rectangles; production keeps one
    // topology allocation shared by all packets.
    #[cfg(test)]
    pub rectangles: Vec<MaskRect>,
    pub(super) geometry: Arc<[MaskRect]>,
    pub local_mesh: Arc<LocalMesh>,
}
#[cfg(test)]
#[derive(Clone, Debug, Default)]
pub struct ResidueStats {
    pub pixel_count: usize,
    pub max_alpha: u8,
    pub bounds: Option<[u32; 4]>,
}
#[derive(Clone, Debug)]
pub struct CompiledModel {
    pub definition: ModelDefinition,
    pub parts: Vec<CompiledPart>,
    /// Row-major local tile coordinates; None is source alpha zero.
    #[cfg(test)]
    pub owner_map: Vec<Option<usize>>,
    #[cfg(test)]
    pub residue_stats: ResidueStats,
    pub bind: super::bind::CompiledBind,
}
#[cfg(test)]
impl CompiledModel {
    pub fn owner_at(&self, x: u32, y: u32) -> Option<&CompiledPart> {
        let t = self.definition.source.tile;
        if x < t[0] || x >= t[2] || y < t[1] || y >= t[3] {
            return None;
        }
        self.owner_map[((y - t[1]) * (t[2] - t[0]) + (x - t[0])) as usize]
            .map(|index| &self.parts[index])
    }
}
#[derive(Clone, Debug)]
pub struct CompiledCatalogue {
    pub definition: PlayerModelCatalogue,
    pub sources: BTreeMap<String, CompiledSource>,
    pub models: BTreeMap<String, CompiledModel>,
    pub attachments: BTreeMap<String, super::attachment_compile::CompiledAttachment>,
}
impl CompiledCatalogue {
    pub fn source(&self, id: &str) -> Option<&CompiledSource> {
        self.sources.get(id)
    }
    pub fn model(&self, id: &str) -> Option<&CompiledModel> {
        self.models.get(id)
    }
}
pub fn rgba_fnv1a64(bytes: &[u8]) -> String {
    let mut value = 0xcbf29ce484222325_u64;
    for byte in bytes {
        value ^= u64::from(*byte);
        value = value.wrapping_mul(0x100000001b3);
    }
    format!("{value:016x}")
}
pub fn compile_catalogue(
    definition: PlayerModelCatalogue,
    views: &[SourceImageView<'_>],
) -> Result<CompiledCatalogue, String> {
    definition.validate()?;
    if views.len() > 16 {
        return Err("Too many player source images".into());
    }
    let mut input = BTreeMap::new();
    for view in views {
        if input.insert(view.id, *view).is_some() {
            return Err(format!("Duplicate source view {}", view.id));
        }
    }
    let mut sources = BTreeMap::new();
    let wanted_sources = definition
        .models
        .iter()
        .map(|m| super::attachments::SourceAssetDefinition::from(&m.source))
        .chain(definition.sources.iter().cloned())
        .collect::<Vec<_>>();
    for wanted in &wanted_sources {
        if sources.contains_key(&wanted.id) {
            continue;
        }
        let source = input
            .get(wanted.id.as_str())
            .ok_or_else(|| format!("Missing source pixels {}", wanted.id))?;
        if [source.width, source.height] != wanted.size {
            return Err(format!("Invalid RGBA dimensions {}", wanted.id));
        }
        let bytes = u64::from(source.width) * u64::from(source.height) * 4;
        if bytes != source.rgba.len() as u64 {
            return Err(format!("Invalid RGBA dimensions/bytes {}", wanted.id));
        }
        let hash = rgba_fnv1a64(source.rgba);
        if !hash.eq_ignore_ascii_case(&wanted.rgba_fnv1a64) {
            return Err(format!("Source RGBA hash mismatch {}", wanted.id));
        }
        sources.insert(
            wanted.id.clone(),
            CompiledSource {
                id: wanted.id.clone(),
                path: wanted.path.clone(),
                width: source.width,
                height: source.height,
                rgba: Arc::from(source.rgba),
                rgba_fnv1a64: hash,
                png_sha256: wanted.png_sha256.clone(),
            },
        );
    }
    if input.keys().any(|id| !sources.contains_key(*id)) {
        return Err("Unknown extra player source view".into());
    }
    let mut models = BTreeMap::new();
    for model in &definition.models {
        let source = &sources[&model.source.id];
        let compiled = compile_model(model, source)?;
        models.insert(model.id.clone(), compiled);
    }
    let mut attachments = BTreeMap::new();
    for attachment in &definition.attachments {
        let source = sources
            .get(&attachment.source_id)
            .ok_or("Missing attachment source")?;
        let compiled = super::attachment_compile::compile_attachment(attachment, source)?;
        attachments.insert(attachment.id.clone(), compiled);
    }
    let compiled = CompiledCatalogue {
        definition,
        sources,
        models,
        attachments,
    };
    super::attachment_compile::validate_support_surfaces(&compiled)?;
    Ok(compiled)
}
fn local_index(x: u32, y: u32, t: [u32; 4]) -> usize {
    ((y - t[1]) * (t[2] - t[0]) + (x - t[0])) as usize
}
fn compile_model(
    model: &ModelDefinition,
    source: &CompiledSource,
) -> Result<CompiledModel, String> {
    let tile = model.source.tile;
    let pixels = ((tile[2] - tile[0]) as usize) * ((tile[3] - tile[1]) as usize);
    let mut owner = vec![None; pixels];
    let mut hidden: BTreeMap<usize, Vec<bool>> = BTreeMap::new();
    let mut raster_work = 0_u64;
    let mut remainder = None;
    for (index, part) in model.parts.iter().enumerate() {
        if part.remainder_max_alpha.is_some()
            && part.source_runs.is_empty()
            && part.authoring_polygons.is_empty()
        {
            remainder = Some(index);
            continue;
        }
        if part.channel == SourceChannel::HiddenFill {
            hidden.insert(index, vec![false; pixels]);
        }
        let mut assign = |x: u32, y: u32| -> Result<(), String> {
            let pixel = source.pixel(x, y).expect("Validated source run");
            if pixel[3] == 0 {
                return Ok(());
            }
            if part.channel == SourceChannel::Residue
                && pixel[3] > part.remainder_max_alpha.unwrap_or(0)
            {
                return Err(format!("Non-faint residue pixel {} ({x},{y})", model.id));
            }
            let at = local_index(x, y, tile);
            if let Some(mask) = hidden.get_mut(&index) {
                if mask[at] && model.mask_mode == MaskMode::Runs {
                    return Err("Duplicate hidden-fill run texel".into());
                }
                mask[at] = true;
                return Ok(());
            }
            match owner[at] {
                None => owner[at] = Some(index),
                Some(previous) => {
                    if model.mask_mode == MaskMode::Runs {
                        return Err(format!(
                            "Duplicate visible ownership {} ({x},{y}): {} / {}",
                            model.id, model.parts[previous].id, part.id
                        ));
                    }
                    if previous == index {
                        return Ok(());
                    }
                    let old = model.parts[previous].priority;
                    if old == part.priority {
                        return Err(format!(
                            "Ambiguous equal-priority polygon owners {} / {}",
                            model.parts[previous].id, part.id
                        ));
                    }
                    if part.priority > old {
                        owner[at] = Some(index);
                    }
                }
            }
            Ok(())
        };
        match model.mask_mode {
            MaskMode::Runs => {
                for [y, x0, x1] in &part.source_runs {
                    raster_work += u64::from(x1 - x0);
                    if raster_work > pixels as u64 * 4 {
                        return Err("Source run work budget exceeded".into());
                    }
                    for x in *x0..*x1 {
                        assign(x, *y)?;
                    }
                }
            }
            MaskMode::PriorityPolygons => {
                for polygon in &part.authoring_polygons {
                    let min_x = polygon
                        .iter()
                        .map(|p| p[0])
                        .fold(f32::INFINITY, f32::min)
                        .floor() as u32;
                    let max_x = polygon.iter().map(|p| p[0]).fold(0., f32::max).ceil() as u32;
                    let min_y = polygon
                        .iter()
                        .map(|p| p[1])
                        .fold(f32::INFINITY, f32::min)
                        .floor() as u32;
                    let max_y = polygon.iter().map(|p| p[1]).fold(0., f32::max).ceil() as u32;
                    raster_work +=
                        u64::from(max_x - min_x) * u64::from(max_y - min_y) * polygon.len() as u64;
                    if raster_work > 200_000_000 {
                        return Err("Polygon compilation work budget exceeded".into());
                    }
                    for y in min_y..max_y {
                        for x in min_x..max_x {
                            if inside_polygon([x as f32 + 0.5, y as f32 + 0.5], polygon) {
                                assign(x, y)?;
                            }
                        }
                    }
                }
            }
        }
    }
    #[cfg(test)]
    let mut residue_stats = ResidueStats::default();
    for y in tile[1]..tile[3] {
        for x in tile[0]..tile[2] {
            let alpha = source.pixel(x, y).expect("Source tile")[3];
            if alpha == 0 {
                continue;
            }
            let at = local_index(x, y, tile);
            if owner[at].is_none() {
                let index = remainder.ok_or_else(|| {
                    format!("Unowned source texel {} ({x},{y}), alpha {alpha}", model.id)
                })?;
                if alpha > model.parts[index].remainder_max_alpha.unwrap_or(0) {
                    return Err(format!(
                        "Unowned anatomical texel {} ({x},{y}), alpha {alpha}",
                        model.id
                    ));
                }
                owner[at] = Some(index);
            }
            #[cfg(test)]
            if model.parts[owner[at].unwrap()].channel == SourceChannel::Residue {
                residue_stats.pixel_count += 1;
                residue_stats.max_alpha = residue_stats.max_alpha.max(alpha);
                let b = residue_stats.bounds.get_or_insert([x, y, x + 1, y + 1]);
                b[0] = b[0].min(x);
                b[1] = b[1].min(y);
                b[2] = b[2].max(x + 1);
                b[3] = b[3].max(y + 1);
            }
        }
    }
    let mut runs = vec![Vec::new(); model.parts.len()];
    for y in tile[1]..tile[3] {
        let mut x = tile[0];
        while x < tile[2] {
            let begin = x;
            let who = owner[local_index(x, y, tile)];
            x += 1;
            while x < tile[2] && owner[local_index(x, y, tile)] == who {
                x += 1;
            }
            let run = MaskRun {
                y,
                x0: begin,
                x1: x,
            };
            if let Some(index) = who {
                runs[index].push(run);
            }
        }
    }
    for (index, mask) in hidden {
        for y in tile[1]..tile[3] {
            let mut x = tile[0];
            while x < tile[2] {
                if !mask[local_index(x, y, tile)] {
                    x += 1;
                    continue;
                }
                let begin = x;
                x += 1;
                while x < tile[2] && mask[local_index(x, y, tile)] {
                    x += 1;
                }
                runs[index].push(MaskRun {
                    y,
                    x0: begin,
                    x1: x,
                });
            }
        }
    }
    let mut count = 0;
    let mut parts = Vec::with_capacity(model.parts.len());
    for (definition, runs) in model.parts.iter().cloned().zip(runs) {
        count += runs.len();
        if count > 150_000 {
            return Err("Compiled mask run budget exceeded".into());
        }
        let rectangles = coalesce(&runs);
        let geometry = Arc::from(rectangles.as_slice());
        let local_mesh = Arc::new(compile_geometry(&rectangles, source.width, source.height));
        parts.push(CompiledPart {
            definition,
            #[cfg(test)]
            runs,
            #[cfg(test)]
            rectangles,
            geometry,
            local_mesh,
        });
    }
    Ok(CompiledModel {
        definition: model.clone(),
        parts,
        #[cfg(test)]
        owner_map: owner,
        #[cfg(test)]
        residue_stats,
        bind: super::bind::compile_bind(model)?,
    })
}
pub(super) fn compile_geometry(rectangles: &[MaskRect], width: u32, height: u32) -> LocalMesh {
    let mut vertices = Vec::with_capacity(rectangles.len() * 4);
    let mut indices = Vec::with_capacity(rectangles.len() * 6);
    for r in rectangles {
        let start = vertices.len() as u32;
        for point in [[r.x0, r.y0], [r.x1, r.y0], [r.x1, r.y1], [r.x0, r.y1]] {
            vertices.push(SourceVertex {
                position: [point[0] as f32, point[1] as f32],
                uv: [
                    point[0] as f32 / width as f32,
                    point[1] as f32 / height as f32,
                ],
            });
        }
        indices.extend([start, start + 1, start + 2, start, start + 2, start + 3]);
    }
    LocalMesh { vertices, indices }
}
fn inside_polygon(p: [f32; 2], polygon: &[[f32; 2]]) -> bool {
    let mut inside = false;
    let mut previous = polygon.len() - 1;
    for i in 0..polygon.len() {
        let a = polygon[i];
        let b = polygon[previous];
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            inside = !inside;
        }
        previous = i;
    }
    inside
}
pub(super) fn coalesce(runs: &[MaskRun]) -> Vec<MaskRect> {
    let mut rectangles: Vec<MaskRect> = Vec::new();
    let mut last: BTreeMap<(u32, u32), usize> = BTreeMap::new();
    for run in runs {
        if let Some(index) = last.get(&(run.x0, run.x1)).copied()
            && rectangles[index].y1 == run.y
        {
            rectangles[index].y1 = run.y + 1;
            continue;
        }
        let index = rectangles.len();
        rectangles.push(MaskRect {
            x0: run.x0,
            y0: run.y,
            x1: run.x1,
            y1: run.y + 1,
        });
        last.insert((run.x0, run.x1), index);
    }
    rectangles
}
