//! Interpolation between ticks: visuals lerp `PrevSimPos → SimPos` by how
//! far the driver's own accumulator ([`SimTime`]) is into the next tick.
//! Floats are fine here; they never flow back into the sim.

use bevy::prelude::*;
use npc_kernel::{SimControl, SimRate, SimTime};

/// Fraction of the current tick to interpolate by, `0.0..=1.0`: 1 while the
/// sim is not advancing (so a paused sim shows its latest state without
/// jitter) or when no driver runs.
pub fn interp_alpha(
    time: Option<&SimTime>,
    rate: Option<&SimRate>,
    control: Option<&SimControl>,
) -> f32 {
    if control.is_none_or(SimControl::is_stopped) {
        return 1.0;
    }
    match (time, rate) {
        (Some(t), Some(r)) => t.overstep_ppm(r) as f32 / 1_000_000.0,
        _ => 1.0,
    }
}

/// [`interp_alpha`] from the world's resources.
pub fn alpha(
    time: Option<Res<SimTime>>,
    rate: Option<Res<SimRate>>,
    control: Option<Res<SimControl>>,
) -> f32 {
    interp_alpha(time.as_deref(), rate.as_deref(), control.as_deref())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn alpha_follows_the_accumulator() {
        let rate = SimRate { hz: 20 };
        let running = SimControl::default();
        let time = SimTime {
            accumulated: Duration::from_millis(25),
            ..Default::default()
        };
        assert_eq!(interp_alpha(Some(&time), Some(&rate), Some(&running)), 0.5);
        let paused = SimControl {
            paused: true,
            ..Default::default()
        };
        assert_eq!(interp_alpha(Some(&time), Some(&rate), Some(&paused)), 1.0);
        assert_eq!(interp_alpha(None, None, None), 1.0);
    }
}
