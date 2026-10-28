//! Small bounded raster plots for native Bevy UI. Values and timestamps come from the collector.
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use npc_perf::{HistoryChart, SeriesRole};

const WIDTH: u32 = 800;
const HEIGHT: u32 = 120;
pub(crate) fn color(role: SeriesRole) -> [u8; 4] {
    match role {
        SeriesRole::Actual => [110, 199, 240, 255],
        SeriesRole::Peak => [255, 148, 133, 255],
        SeriesRole::Target => [242, 204, 115, 255],
    }
}
pub(crate) fn ceiling(chart: &HistoryChart) -> f64 {
    let max = chart
        .series
        .iter()
        .flat_map(|s| s.values.iter().flatten())
        .copied()
        .filter(|v| v.is_finite() && *v >= 0.0)
        .fold(0.0, f64::max);
    if max == 0.0 {
        return 1.0;
    }
    let decade = 10_f64.powf(max.log10().floor());
    [1.0, 2.0, 5.0, 10.0]
        .into_iter()
        .find(|m| m * decade >= max * 1.05)
        .unwrap_or(10.0)
        * decade
}
pub(crate) fn image(chart: &HistoryChart, times: &[f64]) -> Image {
    let mut pixels = vec![0; (WIDTH * HEIGHT * 4) as usize];
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel.copy_from_slice(&[20, 25, 33, 255]);
    }
    let mut put = |x: i32, y: i32, rgba: [u8; 4]| {
        if x >= 0 && y >= 0 && x < WIDTH as i32 && y < HEIGHT as i32 {
            let at = ((y as u32 * WIDTH + x as u32) * 4) as usize;
            pixels[at..at + 4].copy_from_slice(&rgba);
        }
    };
    for row in 0..=4 {
        let y = 3 + row * (HEIGHT as i32 - 7) / 4;
        for x in 3..WIDTH as i32 - 3 {
            put(x, y, [49, 59, 72, 255]);
        }
    }
    let last = times.last().copied().unwrap_or(0.0);
    let first = times.first().copied().unwrap_or(0.0);
    let span = (last - first).max(npc_perf::HISTORY_PERIOD_SECS);
    let top = ceiling(chart);
    // Targets first, so a coincident achieved line remains visible.
    for role in [SeriesRole::Target, SeriesRole::Peak, SeriesRole::Actual] {
        for series in chart.series.iter().filter(|s| s.role == role) {
            let mut previous: Option<(i32, i32, f64)> = None;
            for (&time, value) in times.iter().zip(&series.values) {
                let Some(value) = value.filter(|v| v.is_finite() && *v >= 0.0) else {
                    previous = None;
                    continue;
                };
                let x = 3 + ((time - first) / span * f64::from(WIDTH - 7)).round() as i32;
                let y = HEIGHT as i32 - 4 - (value / top * f64::from(HEIGHT - 7)).round() as i32;
                if let Some((px, py, before)) = previous
                    && time - before <= npc_perf::HISTORY_PERIOD_SECS * 3.0
                {
                    let steps = (x - px).abs().max((y - py).abs()).max(1);
                    for i in 0..=steps {
                        let dx = px + (x - px) * i / steps;
                        let dy = py + (y - py) * i / steps;
                        if role != SeriesRole::Target || dx % 10 < 6 {
                            put(dx, dy, color(role));
                            put(dx, dy - 1, color(role));
                        }
                    }
                } else {
                    put(x, y, color(role));
                }
                previous = Some((x, y, time));
            }
        }
    }
    Image::new(
        Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn plot_scales_cover_peaks_and_missing_values_do_not_draw_a_zero_line() {
        let mut chart = HistoryChart {
            title: "test".into(),
            unit: "us".into(),
            series: vec![npc_perf::HistorySeries {
                name: "self".into(),
                role: SeriesRole::Actual,
                values: vec![Some(20.0), None, Some(80.0)],
            }],
        };
        assert_eq!(ceiling(&chart), 100.0);
        let drawn = image(&chart, &[0.0, 0.5, 1.0]);
        let count = |image: &Image| {
            image
                .data
                .as_ref()
                .unwrap()
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|p| **p == color(SeriesRole::Actual))
                .count()
        };
        assert_eq!(count(&drawn), 2);
        chart.series[0].values[1] = Some(40.0);
        assert!(count(&image(&chart, &[0.0, 0.5, 1.0])) > 100);
    }
}
