//! One shared source-to-display mapping. Bind transforms do not fit or stretch parts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModelPlacement {
    pub source_origin: [f32; 2],
    pub scale: f32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourceToDisplay {
    pub source_origin: [f32; 2],
    pub source_ground: [f32; 2],
    pub scale: f32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine2 {
    pub xx: f32,
    pub xy: f32,
    pub yx: f32,
    pub yy: f32,
    pub tx: f32,
    pub ty: f32,
}
impl Affine2 {
    pub const IDENTITY: Self = Self {
        xx: 1.,
        xy: 0.,
        yx: 0.,
        yy: 1.,
        tx: 0.,
        ty: 0.,
    };
    pub fn point(self, p: [f32; 2]) -> [f32; 2] {
        [
            self.xx * p[0] + self.xy * p[1] + self.tx,
            self.yx * p[0] + self.yy * p[1] + self.ty,
        ]
    }
    /// self(other(point))
    pub fn compose(self, b: Self) -> Self {
        Self {
            xx: self.xx * b.xx + self.xy * b.yx,
            xy: self.xx * b.xy + self.xy * b.yy,
            yx: self.yx * b.xx + self.yy * b.yx,
            yy: self.yx * b.xy + self.yy * b.yy,
            tx: self.xx * b.tx + self.xy * b.ty + self.tx,
            ty: self.yx * b.tx + self.yy * b.ty + self.ty,
        }
    }
    pub fn inverse(self) -> Result<Self, String> {
        let determinant = self.xx * self.yy - self.xy * self.yx;
        if !determinant.is_finite() || determinant.abs() < 1e-8 {
            return Err("Singular player bone transform".into());
        }
        let xx = self.yy / determinant;
        let xy = -self.xy / determinant;
        let yx = -self.yx / determinant;
        let yy = self.xx / determinant;
        Ok(Self {
            xx,
            xy,
            yx,
            yy,
            tx: -xx * self.tx - xy * self.ty,
            ty: -yx * self.tx - yy * self.ty,
        })
    }
    pub fn translation(p: [f32; 2]) -> Self {
        Self {
            tx: p[0],
            ty: p[1],
            ..Self::IDENTITY
        }
    }
    pub fn around(origin: [f32; 2], angle: f32) -> Self {
        if angle == 0. {
            return Self::IDENTITY;
        }
        let (s, c) = angle.sin_cos();
        Self {
            xx: c,
            xy: -s,
            yx: s,
            yy: c,
            tx: origin[0] - c * origin[0] + s * origin[1],
            ty: origin[1] - s * origin[0] - c * origin[1],
        }
    }
    pub fn mirror_x(x: f32) -> Self {
        Self {
            xx: -1.,
            tx: 2. * x,
            ..Self::IDENTITY
        }
    }
    pub fn from_source_binding(b: super::attachments::SourceToBind) -> Self {
        let (s, c) = b.rotation.sin_cos();
        let xx = c * b.scale;
        let xy = -s * b.scale;
        let yx = s * b.scale;
        let yy = c * b.scale;
        Self {
            xx,
            xy,
            yx,
            yy,
            tx: b.bind_origin[0] - xx * b.source_origin[0] - xy * b.source_origin[1],
            ty: b.bind_origin[1] - yx * b.source_origin[0] - yy * b.source_origin[1],
        }
    }
}
impl SourceToDisplay {
    pub fn point(&self, source: [f32; 2]) -> [f32; 2] {
        [
            (source[0] - self.source_ground[0]) * self.scale + self.source_origin[0],
            (source[1] - self.source_ground[1]) * self.scale + self.source_origin[1],
        ]
    }
    pub fn inverse_point(&self, p: [f32; 2]) -> [f32; 2] {
        [
            (p[0] - self.source_origin[0]) / self.scale + self.source_ground[0],
            (p[1] - self.source_origin[1]) / self.scale + self.source_ground[1],
        ]
    }
}
impl ModelPlacement {
    pub(super) fn at_ground(self, source_ground: [f32; 2]) -> Result<SourceToDisplay, String> {
        if !self.scale.is_finite()
            || self.scale <= 0.
            || self.scale > 64.
            || self
                .source_origin
                .iter()
                .any(|v| !v.is_finite() || v.abs() > 10_000_000.)
        {
            return Err("Invalid player model placement".into());
        }
        Ok(SourceToDisplay {
            source_origin: self.source_origin,
            source_ground,
            scale: self.scale,
        })
    }
}

#[derive(Clone, Debug)]
pub struct BoundBone {
    pub id: String,
    pub parent: Option<usize>,
    pub origin: [f32; 2],
    pub tip: Option<[f32; 2]>,
    pub length: Option<f32>,
}
#[derive(Clone, Debug)]
pub struct CompiledBind {
    pub bones: Vec<BoundBone>,
    pub indices: std::collections::BTreeMap<String, usize>,
}
pub(super) fn compile_bind(model: &super::ModelDefinition) -> Result<CompiledBind, String> {
    let mut bones = Vec::with_capacity(model.bones.len());
    let mut indices = std::collections::BTreeMap::new();
    while bones.len() < model.bones.len() {
        let before = bones.len();
        for bone in &model.bones {
            if indices.contains_key(&bone.id) {
                continue;
            }
            let parent = if let Some(parent) = &bone.parent {
                let Some(index) = indices.get(parent).copied() else {
                    continue;
                };
                Some(index)
            } else {
                None
            };
            let length = bone
                .bind_tip
                .map(|tip| (tip[0] - bone.bind_origin[0]).hypot(tip[1] - bone.bind_origin[1]));
            indices.insert(bone.id.clone(), bones.len());
            bones.push(BoundBone {
                id: bone.id.clone(),
                parent,
                origin: bone.bind_origin,
                tip: bone.bind_tip,
                length,
            });
        }
        if bones.len() == before {
            return Err("Cannot compile cyclic/disconnected player bind".into());
        }
    }
    Ok(CompiledBind { bones, indices })
}
