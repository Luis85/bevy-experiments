//! Small, reproducible authored micro-surface maps. No downloaded imagery and
//! no runtime texture synthesis after startup. Complete mip chains prevent
//! high-frequency ground detail from shimmering at landscape distances.
use bevy::{
    asset::RenderAssetUsages,
    image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

fn noise(x: f32, y: f32, cells: u32, seed: u32) -> f32 {
    let x = x * cells as f32;
    let y = y * cells as f32;
    let hash = |x: u32, y: u32| {
        let mut h =
            (x % cells).wrapping_mul(374761393) ^ (y % cells).wrapping_mul(668265263) ^ seed;
        h = (h ^ (h >> 13)).wrapping_mul(1274126177);
        (h & 65535) as f32 / 65535.
    };
    let smooth = |t: f32| t * t * (3. - 2. * t);
    let fx = smooth(x.fract());
    let fy = smooth(y.fract());
    let ix = x.floor() as u32;
    let iy = y.floor() as u32;
    let a = hash(ix, iy) * (1. - fx) + hash(ix + 1, iy) * fx;
    let b = hash(ix, iy + 1) * (1. - fx) + hash(ix + 1, iy + 1) * fx;
    a * (1. - fy) + b * fy
}

pub(super) fn terrain_detail_images() -> (Image, Image) {
    const SIDE: usize = 256;
    let mut heights = vec![0.; SIDE * SIDE];
    let mut albedo = Vec::with_capacity(SIDE * SIDE * 4);
    for y in 0..SIDE {
        for x in 0..SIDE {
            let u = x as f32 / SIDE as f32;
            let v = y as f32 / SIDE as f32;
            let organic = noise(u, v, 8, 73) * 0.38
                + noise(u, v, 32, 531) * 0.32
                + noise(u, v, 128, 917) * 0.30;
            heights[y * SIDE + x] = organic;
            let value = (187. + organic * 62.) as u8;
            albedo.extend([value, value.saturating_add(3), value.saturating_sub(5), 255]);
        }
    }
    (
        mip_image(SIDE, albedo, false),
        mip_image(SIDE, normal_bytes(SIDE, &heights, 1.5), true),
    )
}

pub(super) fn water_normal_image() -> Image {
    const SIDE: usize = 128;
    let tau = std::f32::consts::TAU;
    let mut heights = Vec::with_capacity(SIDE * SIDE);
    for y in 0..SIDE {
        for x in 0..SIDE {
            let u = x as f32 / SIDE as f32 * tau;
            let v = y as f32 / SIDE as f32 * tau;
            heights.push(
                (u * 5. + (v * 3.).sin() * 0.6).sin() * 0.35
                    + (v * 9. + u * 2.).sin() * 0.15
                    + (u * 13. - v * 7.).sin() * 0.07,
            );
        }
    }
    mip_image(SIDE, normal_bytes(SIDE, &heights, 1.2), true)
}

fn normal_bytes(side: usize, heights: &[f32], strength: f32) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(side * side * 4);
    for y in 0..side {
        for x in 0..side {
            let dx = (heights[y * side + (x + 1) % side]
                - heights[y * side + (x + side - 1) % side])
                * strength;
            let dy = (heights[((y + 1) % side) * side + x]
                - heights[((y + side - 1) % side) * side + x])
                * strength;
            let normal = Vec3::new(-dx, -dy, 1.).normalize();
            bytes.extend(normal.to_array().map(|n| ((n * 0.5 + 0.5) * 255.) as u8));
            bytes.push(255);
        }
    }
    bytes
}

fn mip_image(side: usize, pixels: Vec<u8>, normal: bool) -> Image {
    let mut image = Image::new(
        Extent3d {
            width: side as u32,
            height: side as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels.clone(),
        if normal {
            TextureFormat::Rgba8Unorm
        } else {
            TextureFormat::Rgba8UnormSrgb
        },
        RenderAssetUsages::RENDER_WORLD,
    );
    let mut bytes = pixels.clone();
    let mut previous = pixels;
    let mut width = side;
    let mut levels = 1;
    while width > 1 {
        let next_width = width / 2;
        let mut next = Vec::with_capacity(next_width * next_width * 4);
        for y in 0..next_width {
            for x in 0..next_width {
                let mut rgba = [0f32; 4];
                for dy in 0..2 {
                    for dx in 0..2 {
                        let i = ((y * 2 + dy) * width + x * 2 + dx) * 4;
                        for channel in 0..4 {
                            rgba[channel] += previous[i + channel] as f32 * 0.25;
                        }
                    }
                }
                if normal {
                    let n = (Vec3::new(rgba[0], rgba[1], rgba[2]) / 255. * 2. - Vec3::ONE)
                        .normalize_or_zero();
                    for (i, component) in n.to_array().into_iter().enumerate() {
                        rgba[i] = (component * 0.5 + 0.5) * 255.;
                    }
                }
                next.extend(rgba.map(|c| c.round().clamp(0., 255.) as u8));
            }
        }
        bytes.extend_from_slice(&next);
        previous = next;
        width = next_width;
        levels += 1;
    }
    image.data = Some(bytes);
    image.texture_descriptor.mip_level_count = levels;
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        anisotropy_clamp: 4,
        ..ImageSamplerDescriptor::linear()
    });
    image
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authored_materials_have_complete_bounded_mips_and_normal_encoding() {
        let (albedo, normal) = terrain_detail_images();
        let water = water_normal_image();
        for image in [&albedo, &normal, &water] {
            let mut width = image.texture_descriptor.size.width as usize;
            let mut expected = 0;
            let mut levels = 0;
            loop {
                expected += width * width * 4;
                levels += 1;
                if width == 1 {
                    break;
                }
                width /= 2;
            }
            assert_eq!(image.data.as_ref().unwrap().len(), expected);
            assert_eq!(image.texture_descriptor.mip_level_count, levels);
            assert!(expected < 350_000);
        }
        assert_eq!(normal.texture_descriptor.format, TextureFormat::Rgba8Unorm);
        for pixel in normal.data.as_ref().unwrap().chunks_exact(4) {
            let n = Vec3::new(pixel[0] as f32, pixel[1] as f32, pixel[2] as f32) / 255. * 2.
                - Vec3::ONE;
            assert!((n.length() - 1.).abs() < 0.02);
            assert!(n.z > 0.);
        }
        assert_eq!(terrain_detail_images().0.data, albedo.data);
    }
}
