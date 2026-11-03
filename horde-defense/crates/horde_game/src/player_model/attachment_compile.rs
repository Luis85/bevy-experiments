//! Cache additive source geometry without changing original ownership maps.
use super::attachments::AttachmentDefinition;
use super::{CompiledSource, LocalMesh, MaskRect, MaskRun};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct CompiledAttachment {
    pub definition: AttachmentDefinition,
    pub runs: Vec<MaskRun>,
    pub local_mesh: Arc<LocalMesh>,
    pub binding: super::bind::Affine2,
    pub(super) geometry: Arc<[MaskRect]>,
}
pub(super) fn compile_attachment(
    definition: &AttachmentDefinition,
    source: &CompiledSource,
) -> Result<CompiledAttachment, String> {
    let mut pixels = Vec::new();
    let mut work = 0_usize;
    for [y, x0, x1] in &definition.source_runs {
        work = work
            .checked_add((x1 - x0) as usize)
            .ok_or("Attachment work overflow")?;
        if work > 32_000_000 {
            return Err("Attachment source work budget exceeded".into());
        }
        for x in *x0..*x1 {
            if source.pixel(x, *y).ok_or("Invalid attachment texel")?[3] > 0 {
                pixels.push(*y as usize * source.width as usize + x as usize);
            }
        }
    }
    pixels.sort_unstable();
    if pixels.windows(2).any(|p| p[0] == p[1]) {
        return Err(format!(
            "Duplicate attachment source pixels {}",
            definition.id
        ));
    }
    let mut runs = Vec::new();
    let mut i = 0;
    while i < pixels.len() {
        let start = pixels[i];
        let y = (start / source.width as usize) as u32;
        let x0 = (start % source.width as usize) as u32;
        i += 1;
        let mut x1 = x0 + 1;
        while i < pixels.len()
            && pixels[i] / source.width as usize == y as usize
            && pixels[i] % source.width as usize == x1 as usize
        {
            i += 1;
            x1 += 1;
        }
        runs.push(MaskRun { y, x0, x1 });
    }
    let rectangles = super::source_masks::coalesce(&runs);
    let geometry = Arc::from(rectangles.as_slice());
    let local_mesh = Arc::new(super::source_masks::compile_geometry(
        &rectangles,
        source.width,
        source.height,
    ));
    let binding = super::bind::Affine2::from_source_binding(definition.source_to_bind);
    Ok(CompiledAttachment {
        definition: definition.clone(),
        runs,
        geometry,
        local_mesh,
        binding,
    })
}
pub(super) fn validate_support_surfaces(c: &super::CompiledCatalogue) -> Result<(), String> {
    let mut work = 0_usize;
    for track in &c.definition.support_tracks {
        let attachment = c
            .attachments
            .get(&track.weapon_attachment)
            .ok_or("Missing support weapon")?;
        let source = c
            .source(&attachment.definition.source_id)
            .ok_or("Missing support source")?;
        let eligible = |p: [f32; 2]| {
            if p[0] < 0. || p[1] < 0. || p[0] >= source.width as f32 || p[1] >= source.height as f32
            {
                return false;
            }
            let x = p[0].floor() as u32;
            let y = p[1].floor() as u32;
            track
                .source_runs
                .iter()
                .any(|r| r[0] == y && r[1] <= x && x < r[2])
                && attachment
                    .runs
                    .iter()
                    .any(|r| r.y == y && r.x0 <= x && x < r.x1)
                && source.pixel(x, y).is_some_and(|rgba| rgba[3] > 0)
        };
        for run in &track.source_runs {
            work += (run[2] - run[1]) as usize;
            if work > 2_000_000 {
                return Err("Support surface budget exceeded".into());
            }
            for x in run[1]..run[2] {
                if !eligible([x as f32 + 0.5, run[0] as f32 + 0.5]) {
                    return Err(format!(
                        "Support surface {} includes nonweapon/transparent pixels",
                        track.id
                    ));
                }
            }
        }
        for key in &track.keys {
            if !eligible(key.source_position) {
                return Err(format!(
                    "Support key {} is outside its actual weapon surface",
                    track.id
                ));
            }
        }
        for pair in track.keys.windows(2) {
            let a = pair[0].source_position;
            let b = pair[1].source_position;
            let d = [b[0] - a[0], b[1] - a[1]];
            let mut cuts = vec![0., 1.];
            for axis in 0..2 {
                if d[axis] != 0. {
                    for border in
                        a[axis].min(b[axis]).floor() as i32..=a[axis].max(b[axis]).ceil() as i32
                    {
                        let t = (border as f32 - a[axis]) / d[axis];
                        if t > 0. && t < 1. {
                            cuts.push(t);
                        }
                    }
                }
            }
            cuts.sort_by(f32::total_cmp);
            cuts.dedup();
            for cut in cuts.windows(2) {
                let t = (cut[0] + cut[1]) * 0.5;
                let p = [a[0] + d[0] * t, a[1] + d[1] * t];
                if !eligible(p) {
                    return Err(format!(
                        "Interpolated support rail {} crosses outside weapon material",
                        track.id
                    ));
                }
            }
        }
    }
    Ok(())
}
