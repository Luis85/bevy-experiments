//! Reusable presentation profiles. These settings never change world authority,
//! source heights, collision, population, or the server's simulation budget.
use super::Sun;
use crate::WorldRuntime;
use bevy::{
    light::{
        Atmosphere, AtmosphereEnvironmentMapLight, CascadeShadowConfig, CascadeShadowConfigBuilder,
        DirectionalLightShadowMap, GeneratedEnvironmentMapLight,
    },
    pbr::{AtmosphereMode, AtmosphereSettings},
    post_process::bloom::Bloom,
    prelude::*,
    render::{
        RenderApp,
        render_resource::{DownlevelFlags, TextureFormat, TextureUsages},
        renderer::{RenderAdapter, RenderDevice},
    },
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderQuality {
    Performance,
    #[default]
    Balanced,
    High,
}
impl RenderQuality {
    pub const fn next(self) -> Self {
        match self {
            Self::Performance => Self::Balanced,
            Self::Balanced => Self::High,
            Self::High => Self::Performance,
        }
    }
    pub const fn label(self) -> &'static str {
        match self {
            Self::Performance => "Performance",
            Self::Balanced => "Balanced",
            Self::High => "High",
        }
    }
    pub fn from_name(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "performance" | "low" => Some(Self::Performance),
            "balanced" | "medium" => Some(Self::Balanced),
            "high" => Some(Self::High),
            _ => None,
        }
    }
    pub const fn shadow_map_size(self) -> usize {
        match self {
            Self::Performance => 1024,
            Self::Balanced => 2048,
            Self::High => 4096,
        }
    }
    pub const fn ground_cluster_limit(self) -> usize {
        match self {
            Self::Performance => 320,
            Self::Balanced => 1024,
            Self::High => 2048,
        }
    }
}

#[derive(Resource, Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RenderQualitySettings {
    pub quality: RenderQuality,
    /// Performance always disables atmospheric scattering and generated IBL.
    /// Disable this explicitly on adapters without the required GPU features.
    pub atmospheric_sky: bool,
    /// Coarsen distant render interiors while retaining full-resolution borders.
    pub terrain_lod: bool,
}
impl Default for RenderQualitySettings {
    fn default() -> Self {
        Self {
            quality: RenderQuality::Balanced,
            atmospheric_sky: true,
            terrain_lod: true,
        }
    }
}
impl RenderQualitySettings {
    pub const fn uses_atmosphere(self) -> bool {
        self.atmospheric_sky && !matches!(self.quality, RenderQuality::Performance)
    }
    pub const fn terrain_detail(self, feature_detail: u8) -> u8 {
        if !self.terrain_lod {
            0
        } else if matches!(self.quality, RenderQuality::High) {
            feature_detail.saturating_sub(1)
        } else {
            feature_detail
        }
    }
}

#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct RenderCapabilities {
    pub atmospheric_sky: bool,
}

pub(super) fn detect_capabilities(app: &mut App) {
    let supported = app.get_sub_app(RenderApp).is_some_and(|render_app| {
        let world = render_app.world();
        let (Some(adapter), Some(device)) = (
            world.get_resource::<RenderAdapter>(),
            world.get_resource::<RenderDevice>(),
        ) else {
            return false;
        };
        adapter
            .get_downlevel_capabilities()
            .flags
            .contains(DownlevelFlags::COMPUTE_SHADERS)
            && adapter
                .get_texture_format_features(TextureFormat::Rgba16Float)
                .allowed_usages
                .contains(TextureUsages::STORAGE_BINDING)
            && device.limits().max_storage_textures_per_shader_stage > 0
    });
    app.insert_resource(RenderCapabilities {
        atmospheric_sky: supported,
    });
    if !supported {
        warn!(
            "Terra atmospheric sky is unavailable on this adapter; using the directional/ambient compatibility path."
        );
    }
}

#[derive(Component)]
pub(super) struct WorldAtmosphere;
#[derive(Component)]
pub(super) struct PresentationCamera(RenderQualitySettings);

fn atmosphere_settings(quality: RenderQuality) -> AtmosphereSettings {
    let high = quality == RenderQuality::High;
    AtmosphereSettings {
        transmittance_lut_size: UVec2::new(if high { 256 } else { 128 }, 32),
        multiscattering_lut_size: UVec2::splat(16),
        sky_view_lut_size: UVec2::new(if high { 256 } else { 128 }, if high { 128 } else { 64 }),
        aerial_view_lut_size: UVec3::splat(if high { 32 } else { 16 }),
        transmittance_lut_samples: if high { 32 } else { 16 },
        multiscattering_lut_dirs: 8,
        multiscattering_lut_samples: 8,
        sky_view_lut_samples: if high { 32 } else { 16 },
        aerial_view_lut_samples: if high { 8 } else { 4 },
        aerial_view_lut_max_distance: 12_000.,
        sky_max_samples: 16,
        rendering_method: AtmosphereMode::LookupTexture,
    }
}

/// Apply GPU-facing profile changes only when the profile or camera changes.
/// Terrain profile changes use the existing bounded background mesh queue.
pub(super) fn configure_presentation(
    mut commands: Commands,
    settings: Res<RenderQualitySettings>,
    capabilities: Res<RenderCapabilities>,
    runtime: Res<WorldRuntime>,
    cameras: Query<
        (
            Entity,
            Option<&PresentationCamera>,
            Option<&bevy::camera::Exposure>,
        ),
        With<Camera3d>,
    >,
    mut shadow_map: ResMut<DirectionalLightShadowMap>,
    mut suns: Query<&mut CascadeShadowConfig, With<Sun>>,
    mut planets: Query<(&Atmosphere, &mut GlobalTransform), With<WorldAtmosphere>>,
) {
    let changed = settings.is_changed();
    let mut effective = *settings;
    effective.atmospheric_sky &= capabilities.atmospheric_sky;
    // The render origin may move; geodetic height must still be the height
    // above the atmosphere's planet rather than above the last camera rebase.
    for (atmosphere, mut transform) in &mut planets {
        let center = -Vec3::Y * (atmosphere.inner_radius + runtime.frame.origin.altitude_m as f32);
        if transform.translation() != center {
            *transform = GlobalTransform::from_translation(center);
        }
    }
    if changed {
        shadow_map.size = settings.quality.shadow_map_size();
        for mut cascades in &mut suns {
            *cascades = CascadeShadowConfigBuilder {
                first_cascade_far_bound: 24.,
                maximum_distance: match settings.quality {
                    RenderQuality::Performance => 600.,
                    RenderQuality::Balanced => 1200.,
                    RenderQuality::High => 2000.,
                },
                ..default()
            }
            .build();
        }
    }
    for (camera, previous, exposure) in &cameras {
        if exposure.is_none() {
            commands
                .entity(camera)
                .insert(bevy::camera::Exposure { ev100: 10.8 });
        }
        if previous.is_some_and(|p| p.0 == effective) {
            continue;
        }
        let mut camera = commands.entity(camera);
        camera.insert((
            PresentationCamera(effective),
            match settings.quality {
                RenderQuality::Performance => Msaa::Off,
                // 2x is optional on common adapters. Use Bevy's portable 4x
                // default instead of assuming an intermediate sample count.
                RenderQuality::Balanced => Msaa::Sample4,
                RenderQuality::High => Msaa::Sample4,
            },
        ));
        if effective.uses_atmosphere() {
            camera.insert((
                atmosphere_settings(settings.quality),
                AtmosphereEnvironmentMapLight {
                    intensity: 1.0,
                    size: UVec2::splat(if settings.quality == RenderQuality::High {
                        64
                    } else {
                        32
                    }),
                    ..default()
                },
            ));
            if settings.quality == RenderQuality::High {
                camera.insert(Bloom {
                    intensity: 0.10,
                    ..Bloom::NATURAL
                });
            } else {
                camera.remove::<Bloom>();
            }
        } else if previous.is_some() {
            camera.remove::<AtmosphereSettings>();
            camera.remove::<AtmosphereEnvironmentMapLight>();
            camera.remove::<GeneratedEnvironmentMapLight>();
            camera.remove::<Bloom>();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_have_bounded_gpu_cost_and_an_explicit_compatibility_path() {
        let mut settings = RenderQualitySettings::default();
        assert!(settings.uses_atmosphere());
        assert_eq!(settings.terrain_detail(2), 2);
        settings.quality = RenderQuality::High;
        assert_eq!(settings.terrain_detail(2), 1);
        settings.atmospheric_sky = false;
        assert!(!settings.uses_atmosphere());
        settings.atmospheric_sky = true;
        settings.quality = RenderQuality::Performance;
        assert!(!settings.uses_atmosphere());
        settings.terrain_lod = false;
        assert_eq!(settings.terrain_detail(2), 0);
        for q in [
            RenderQuality::Performance,
            RenderQuality::Balanced,
            RenderQuality::High,
        ] {
            assert!(q.shadow_map_size() <= 4096);
            assert!(q.ground_cluster_limit() <= 2048);
            let atmosphere = atmosphere_settings(q);
            assert!(atmosphere.aerial_view_lut_size.x <= 32);
            assert!(atmosphere.sky_view_lut_size.x <= 256);
            assert_eq!(RenderQuality::from_name(q.label()), Some(q));
        }
        assert_eq!(
            RenderQuality::Balanced.next().next().next(),
            RenderQuality::Balanced
        );
        assert!(RenderQuality::from_name("ultra-unbounded").is_none());
    }
}
