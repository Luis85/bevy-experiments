//! Bodies on the ground: the cached settled-corpse mesh, then the collapsing
//! bodies.
//!
//! Settled bodies are cached across frames (as the ground meshes of
//! `visual_assets::ground_material` are): one
//! `Arc<Mesh>` per painter layer in egui temp data, keyed by
//! [`CorpseCacheKey`]. While the key holds and the settled prefix only grows,
//! the bodies that settled since the last pass are appended (through the pure
//! [`visual_assets::settled_corpse_quad`]) and the same mesh is painted
//! again; any key change, a shrinking count or a different newest body
//! rebuilds it. Attempt and corpse ids restart every campaign, so the key also
//! holds the run (country, level, mode, arena geometry) and the first and
//! newest bodies are compared by id, tick and position, not by id alone: a
//! loaded save of another run never keeps this run's bodies. Past
//! `CORPSES_SETTLED_DRAWN` (a safety cap, more than 7x the measured maximum
//! per stage) the window slides and each settle rebuilds. Saved
//! `CorpseRecord`s are only read.
use super::super::ArenaProjection;
use super::batch::ActorMeshes;
use super::enemies::tint;
use crate::{
    animation::ActorPose,
    battle_view::{BattleViewModel, CorpseView},
    visual_assets::{self, Actor, ArtTextures},
};
use bevy_egui::egui::{
    self, Color32 as C, Painter, Pos2, Rect, Shape, TextureId, Vec2, epaint::Mesh, vec2,
};
use horde_core::{
    content::Mode,
    sim::{Attempt, CorpseRecord},
};
use std::sync::Arc;

/// A saved body as the cache compares it: ids repeat across runs, so its tick
/// and position are compared too.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct BodyStamp {
    pub id: u64,
    pub tick: u64,
    pub at_bits: [u32; 2],
}
impl BodyStamp {
    fn of(record: &CorpseRecord) -> Self {
        Self {
            id: record.id,
            tick: record.tick,
            at_bits: record.at.map(f32::to_bits),
        }
    }
}

/// What the settled-corpse mesh was built for. Any change rebuilds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CorpseCacheKey {
    pub layer: egui::LayerId,
    /// The attempt the bodies belong to, and its run: attempt ids restart
    /// every campaign.
    pub attempt: u64,
    /// `Id` hash of the campaign's country id.
    pub country: u64,
    pub level: usize,
    pub mode: Mode,
    /// The first drawn body: a new attempt or run, or the drawn window
    /// sliding past `CORPSES_SETTLED_DRAWN`.
    pub first: BodyStamp,
    /// The arena rectangle (resize, zoom, layout) and the projection's lane
    /// count and width in yards.
    pub rect_bits: [u32; 4],
    pub lanes: u8,
    pub width_bits: u32,
    /// The corpse cell's texture and UV (an atlas reload or repack).
    pub texture: TextureId,
    pub uv_bits: [u32; 4],
    pub blood: bool,
}

/// One layer's cached settled bodies.
#[derive(Clone)]
pub(super) struct SettledMesh {
    pub key: CorpseCacheKey,
    /// Bodies in the mesh: `view.settled.count` when it was last painted.
    pub count: usize,
    /// The newest cached body, checked before appending.
    last: Option<BodyStamp>,
    pub mesh: Arc<Mesh>,
    /// From-scratch builds of this layer's mesh; appends do not count.
    pub rebuilds: u64,
    /// Settled-body quads built into this layer's meshes, over every build.
    pub quads_built: u64,
}

fn cache_id(layer: egui::LayerId) -> egui::Id {
    egui::Id::new(("lastlight_settled_corpses", layer))
}

/// The cached settled-corpse mesh of `layer`, for tests and diagnostics.
#[cfg(test)]
pub(super) fn cached(ctx: &egui::Context, layer: egui::LayerId) -> Option<SettledMesh> {
    ctx.data(|data| data.get_temp::<Option<SettledMesh>>(cache_id(layer)))
        .flatten()
}

fn bits(rect: Rect) -> [u32; 4] {
    [rect.min.x, rect.min.y, rect.max.x, rect.max.y].map(f32::to_bits)
}

/// The box a settled body's corpse cell is fitted into.
fn settled_box(arena: ArenaProjection, body: &CorpseView) -> (Pos2, Vec2, C) {
    let height = body.height.pixels(arena.lane_height());
    (
        arena.world(body.at[0], body.at[1]) + vec2(0., height * 0.28),
        vec2(height * 1.25, height * 0.45),
        tint(body.tint).gamma_multiply(0.8),
    )
}

/// Paints the settled bodies of `view` from the layer's cached mesh, adding
/// only the bodies that settled since it was built. `false` when the corpse
/// cell has no texture (missing or rejected actor art).
fn settled(
    p: &Painter,
    arena: ArenaProjection,
    view: &BattleViewModel,
    a: &Attempt,
    textures: ArtTextures,
    blood: bool,
) -> bool {
    let Some((texture, uv)) = visual_assets::settled_corpse_texture(textures) else {
        return false;
    };
    let settled = view.settled;
    // `first_id` is `None` when nothing has settled.
    let Some(first) = settled.first_id.and(a.corpses.get(settled.start)) else {
        return true;
    };
    let key = CorpseCacheKey {
        layer: p.layer_id(),
        attempt: a.id,
        country: egui::Id::new(&a.country_id).value(),
        level: a.level,
        mode: a.mode,
        first: BodyStamp::of(first),
        rect_bits: bits(arena.rect),
        lanes: arena.lanes,
        width_bits: arena.width.to_bits(),
        texture,
        uv_bits: bits(uv),
        blood,
    };
    let id = cache_id(key.layer);
    // Taken out of the context so appending does not copy a shared mesh.
    let cached = p.ctx().data_mut(|data| {
        data.get_temp_mut_or_default::<Option<SettledMesh>>(id)
            .take()
    });
    let newest = |count: usize| {
        let last = (settled.start + count).checked_sub(1)?;
        a.corpses.get(last).map(BodyStamp::of)
    };
    let mut entry = match cached {
        Some(entry)
            if entry.key == key
                && entry.count <= settled.count
                && newest(entry.count) == entry.last =>
        {
            entry
        }
        previous => SettledMesh {
            key,
            count: 0,
            last: None,
            mesh: Arc::new(Mesh::with_texture(texture)),
            rebuilds: previous.as_ref().map_or(0, |entry| entry.rebuilds) + 1,
            quads_built: previous.map_or(0, |entry| entry.quads_built),
        },
    };
    if entry.count < settled.count {
        let mesh = Arc::make_mut(&mut entry.mesh);
        let added = settled.count - entry.count;
        mesh.reserve_vertices(added * 4);
        mesh.reserve_triangles(added * 2);
        for n in entry.count..settled.count {
            let Some(body) = settled.view(a, n) else {
                continue;
            };
            let (center, size, tint) = settled_box(arena, &body);
            if let Some(quad) = visual_assets::settled_corpse_quad(textures, center, size, tint) {
                quad.push(mesh);
            }
        }
        entry.quads_built += added as u64;
        entry.count = settled.count;
        entry.last = newest(settled.count);
    }
    let mesh = Arc::clone(&entry.mesh);
    p.ctx().data_mut(|data| data.insert_temp(id, Some(entry)));
    if !mesh.is_empty() {
        p.add(Shape::Mesh(mesh));
    }
    true
}

/// Paints every body of `view`: the cached settled mesh, the collapsing
/// bodies already past 0.9 collapse as the same corpse cell, then the
/// collapsing sprites in one mesh. Without corpse art, settled bodies draw as
/// collapsed sprites each pass.
pub(super) fn paint(
    p: &Painter,
    arena: ArenaProjection,
    view: &BattleViewModel,
    a: &Attempt,
    textures: ArtTextures,
    blood: bool,
) {
    let corpse_art = settled(p, arena, view, a, textures, blood);
    if corpse_art {
        visual_assets::corpse_batch(
            p,
            textures,
            view.corpses
                .iter()
                .filter(|body| body.collapse > 0.9)
                .map(|body| settled_box(arena, body)),
        );
    }
    let fallback = if corpse_art { 0 } else { view.settled.count };
    let mut collapsing = ActorMeshes::new(textures, view.corpses.len() + fallback);
    for n in 0..fallback {
        if let Some(body) = view.settled.view(a, n) {
            collapsing_body(p, &mut collapsing, arena, &body, textures);
        }
    }
    for body in &view.corpses {
        if !(corpse_art && body.collapse > 0.9) {
            collapsing_body(p, &mut collapsing, arena, body, textures);
        }
    }
    collapsing.paint(p);
}

/// A body's posed sprite into the shared mesh; a legacy body without a
/// figure keeps its ground shadow, under the mesh.
fn collapsing_body(
    p: &Painter,
    mesh: &mut ActorMeshes,
    arena: ArenaProjection,
    body: &CorpseView,
    textures: ArtTextures,
) {
    let sy = arena.lane_height();
    let at = arena.world(body.at[0], body.at[1]);
    let height = body.height.pixels(sy);
    if !body.has_figure {
        p.add(Shape::ellipse_filled(
            at + vec2(0., sy * 0.25),
            vec2(sy * 0.3, sy * 0.10),
            C::from_black_alpha(90),
        ));
        return;
    }
    let Some(sprite) = body.sprite else {
        return;
    };
    let pose = ActorPose {
        frame: body.frame,
        lean: body.lean,
        scale: [1. + body.collapse * 0.16, 1. - body.collapse * 0.72],
        ..Default::default()
    };
    if let Some(quad) = visual_assets::actor_quad(
        textures,
        Actor::Enemy(sprite),
        at,
        vec2(height * 1.2, height),
        tint(body.tint).gamma_multiply(0.8),
        &pose,
    ) {
        mesh.sprite(&quad);
    }
}
