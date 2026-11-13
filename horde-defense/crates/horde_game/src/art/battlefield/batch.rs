//! Layered battlefield meshes. Each layer is one egui mesh built with
//! reserved capacity, so the tessellator emits a fixed number of primitives
//! however many hostiles, shadows, bars and particles are drawn, and none of
//! that geometry is tessellated (or feathered) shape by shape. A builder
//! with a [`MeshBuilder::pixel_floor`] keeps egui's sub-pixel rule instead:
//! a line or dot thinner than a physical pixel is widened to one at reduced
//! opacity, so it neither dashes nor blinks as it moves.
use crate::visual_assets::{
    self, ActorQuad, ActorSheet, ArtTextures, BattleAtlas, FlashMasks, HitFlash,
};
use bevy_egui::egui::{
    Color32 as C, Painter, Pos2, Rect, Shape, TextureId, Vec2,
    epaint::{Mesh, Vertex, WHITE_UV},
    vec2,
};
use std::f32::consts::{FRAC_1_SQRT_2, PI, TAU};

/// The smallest fan radius, in physical pixels, whose inscribed circle (at
/// `cos(pi / segments)` of it, 8 segments the coarsest) reaches the pixel
/// lattice's covering radius `1 / sqrt(2)`: wherever such a fan lands, it
/// covers a pixel centre.
fn min_fan_radius_px(segments: u32) -> f32 {
    FRAC_1_SQRT_2 / (PI / segments.max(8) as f32).cos()
}

/// One mesh under construction. Untextured geometry samples `white`, an
/// opaque white texel of the mesh's texture.
pub(super) struct MeshBuilder {
    mesh: Mesh,
    white: Pos2,
    /// Points per physical pixel when sub-pixel lines and fans are floored,
    /// else 0.0 (drawn as given).
    pixel: f32,
}
impl MeshBuilder {
    /// A mesh on the font atlas, the texture of every egui shape and text.
    pub(super) fn untextured(quads: usize) -> Self {
        Self::with_texture(TextureId::default(), WHITE_UV, quads)
    }
    pub(super) fn with_texture(texture: TextureId, white: Pos2, quads: usize) -> Self {
        let mut mesh = Mesh::with_texture(texture);
        mesh.reserve_vertices(quads * 4);
        mesh.reserve_triangles(quads * 2);
        Self {
            mesh,
            white,
            pixel: 0.0,
        }
    }
    /// Floors [`Self::line`] widths at one physical pixel and [`Self::fan`]
    /// radii at [`min_fan_radius_px`], scaling the colour by the lost width
    /// or area so the coverage is kept (egui's feathered thin-line rule).
    pub(super) fn pixel_floor(mut self, pixels_per_point: f32) -> Self {
        if pixels_per_point > 0.0 {
            self.pixel = 1.0 / pixels_per_point;
        }
        self
    }
    pub(super) fn mesh_mut(&mut self) -> &mut Mesh {
        &mut self.mesh
    }
    fn vertex(&mut self, pos: Pos2, color: C) -> u32 {
        let index = self.mesh.vertices.len() as u32;
        self.mesh.vertices.push(Vertex {
            pos,
            uv: self.white,
            color,
        });
        index
    }
    /// A solid axis-aligned rectangle.
    pub(super) fn quad(&mut self, rect: Rect, color: C) {
        self.mesh
            .add_rect_with_uv(rect, Rect::from_min_max(self.white, self.white), color);
    }
    pub(super) fn quad_uv(&mut self, rect: Rect, uv: Rect, color: C) {
        self.mesh.add_rect_with_uv(rect, uv, color);
    }
    /// A segment of `width` as one 4-vertex quad (no feathering).
    pub(super) fn line(&mut self, a: Pos2, b: Pos2, width: f32, color: C) {
        let (width, color) = if width < self.pixel {
            (self.pixel, color.gamma_multiply(width / self.pixel))
        } else {
            (width, color)
        };
        let normal = (b - a).normalized().rot90() * (width * 0.5);
        let first = self.vertex(a + normal, color);
        self.vertex(a - normal, color);
        self.vertex(b + normal, color);
        self.vertex(b - normal, color);
        self.mesh.add_triangle(first, first + 1, first + 2);
        self.mesh.add_triangle(first + 2, first + 1, first + 3);
    }
    /// A solid triangle.
    pub(super) fn triangle(&mut self, points: [Pos2; 3], color: C) {
        let first = self.vertex(points[0], color);
        self.vertex(points[1], color);
        self.vertex(points[2], color);
        self.mesh.add_triangle(first, first + 1, first + 2);
    }
    /// A filled ellipse of `radii` as a fan of `segments` triangles.
    pub(super) fn fan(&mut self, center: Pos2, radii: Vec2, segments: u32, color: C) {
        let floored = radii.max(Vec2::splat(self.pixel * min_fan_radius_px(segments)));
        let (radii, color) = if floored == radii {
            (radii, color)
        } else {
            let kept = (radii.x * radii.y) / (floored.x * floored.y);
            (floored, color.gamma_multiply(kept))
        };
        let hub = self.vertex(center, color);
        for n in 0..segments {
            let angle = n as f32 / segments as f32 * TAU;
            self.vertex(
                center + vec2(angle.cos() * radii.x, angle.sin() * radii.y),
                color,
            );
        }
        for n in 0..segments {
            self.mesh
                .add_triangle(hub, hub + 1 + n, hub + 1 + (n + 1) % segments);
        }
    }
    /// An ellipse outline of `width` centred on `radii`.
    pub(super) fn ring(&mut self, center: Pos2, radii: Vec2, width: f32, segments: u32, color: C) {
        let half = Vec2::splat(width * 0.5);
        let (outer, inner) = (radii + half, (radii - half).max(Vec2::ZERO));
        let first = self.mesh.vertices.len() as u32;
        for n in 0..segments {
            let angle = n as f32 / segments as f32 * TAU;
            let direction = vec2(angle.cos(), angle.sin());
            self.vertex(center + direction * outer, color);
            self.vertex(center + direction * inner, color);
        }
        for n in 0..segments {
            let (a, b) = (first + 2 * n, first + 2 * ((n + 1) % segments));
            self.mesh.add_triangle(a, a + 1, b);
            self.mesh.add_triangle(b, a + 1, b + 1);
        }
    }
    /// Adds the mesh to `p`, unless nothing was drawn into it.
    pub(super) fn paint(self, p: &Painter) {
        if !self.mesh.is_empty() {
            p.add(Shape::mesh(self.mesh));
        }
    }
}

/// The sprite layer of a battlefield: one packed-atlas mesh that keeps the
/// exact painter order across both sheets, each hit flash one quad right
/// after its sprite, or (when the atlas does not fit the device) a walker
/// mesh, then an actor mesh, then the flash runs.
pub(super) enum ActorMeshes {
    Packed {
        atlas: BattleAtlas,
        mesh: MeshBuilder,
    },
    Split {
        walkers: Option<MeshBuilder>,
        actors: Option<MeshBuilder>,
        flashes: MeshBuilder,
    },
}
impl ActorMeshes {
    pub(super) fn new(textures: ArtTextures, quads: usize) -> Self {
        match textures.battle_atlas {
            Some(atlas) => Self::Packed {
                atlas,
                mesh: MeshBuilder::with_texture(atlas.texture, atlas.white_uv, quads),
            },
            None => Self::Split {
                walkers: textures
                    .walkers
                    .map(|sheet| MeshBuilder::with_texture(sheet.texture, WHITE_UV, quads)),
                actors: textures
                    .actors
                    .map(|texture| MeshBuilder::with_texture(texture, WHITE_UV, quads)),
                flashes: MeshBuilder::untextured(0),
            },
        }
    }
    /// Appends the posed sprite `quad`.
    pub(super) fn sprite(&mut self, quad: &ActorQuad) {
        match self {
            Self::Packed { atlas, mesh } => {
                quad.push(mesh.mesh_mut(), atlas.uv(quad.sheet, quad.uv));
            }
            Self::Split {
                walkers, actors, ..
            } => {
                let target = match quad.sheet {
                    ActorSheet::Walkers => walkers,
                    ActorSheet::Actors => actors,
                };
                if let Some(mesh) = target {
                    quad.push(mesh.mesh_mut(), quad.uv);
                }
            }
        }
    }
    /// Appends the hit flash of the sprite `actor` just pushed and returns
    /// how many quads it took: one posed quad on its packed white silhouette,
    /// or (split meshes) its mask runs.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn flash(
        &mut self,
        masks: &FlashMasks,
        textures: ArtTextures,
        actor: visual_assets::Actor,
        center: Pos2,
        size: Vec2,
        flash: HitFlash,
        pose: &crate::animation::ActorPose,
    ) -> usize {
        match self {
            Self::Packed { atlas, mesh } => visual_assets::hit_flash_quad(
                masks,
                textures,
                actor,
                center,
                size,
                flash,
                pose,
                mesh.mesh_mut(),
                atlas,
            ),
            Self::Split { flashes, .. } => visual_assets::hit_flash_runs(
                masks,
                textures,
                actor,
                center,
                size,
                flash,
                pose,
                flashes.mesh_mut(),
                WHITE_UV,
            ),
        }
    }
    pub(super) fn paint(self, p: &Painter) {
        match self {
            Self::Packed { mesh, .. } => mesh.paint(p),
            Self::Split {
                walkers,
                actors,
                flashes,
            } => {
                for mesh in [walkers, actors].into_iter().flatten() {
                    mesh.paint(p);
                }
                flashes.paint(p);
            }
        }
    }
}

/// The soft-shadow layer: one quad per body on the procedural shadow
/// texture, or the legacy three ellipses each when it has not loaded.
pub(super) struct ShadowMesh {
    mesh: Option<MeshBuilder>,
    legacy: Vec<(Pos2, Vec2, f32)>,
}
impl ShadowMesh {
    pub(super) fn new(textures: ArtTextures, quads: usize) -> Self {
        Self {
            mesh: textures
                .shadow
                .map(|texture| MeshBuilder::with_texture(texture, WHITE_UV, quads)),
            legacy: Vec::new(),
        }
    }
    /// The shadow `art::soft_shadow(at, size, alpha)` draws.
    pub(super) fn shadow(&mut self, at: Pos2, size: Vec2, alpha: f32) {
        match &mut self.mesh {
            Some(mesh) => {
                let (rect, tint) = visual_assets::soft_shadow_quad(at, size, alpha);
                mesh.quad_uv(
                    rect,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    tint,
                );
            }
            None => self.legacy.push((at, size, alpha)),
        }
    }
    pub(super) fn paint(self, p: &Painter) {
        if let Some(mesh) = self.mesh {
            mesh.paint(p);
        }
        for (at, size, alpha) in self.legacy {
            super::super::soft_shadow(p, at, size, alpha);
        }
    }
}

/// The per-hostile world layers of one battlefield, painted back to front:
/// shadows, then sprites. Gauges, rings and labels are cues (`super::cue`).
pub(super) struct FieldMeshes {
    pub(super) shadows: ShadowMesh,
    pub(super) actors: ActorMeshes,
}
impl FieldMeshes {
    pub(super) fn new(textures: ArtTextures, enemies: usize) -> Self {
        Self {
            shadows: ShadowMesh::new(textures, enemies + 1),
            actors: ActorMeshes::new(textures, enemies),
        }
    }
}
