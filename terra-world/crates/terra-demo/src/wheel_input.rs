//! Configurable driving axes from Bevy/Gilrs devices. This adapter submits only
//! bounded ordinary driving intent; it neither owns physics nor sends force feedback.
use bevy::{input::gamepad::GamepadInput, prelude::*};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WheelDevice {
    pub name: String,
    pub vendor: Option<u16>,
    pub product: Option<u16>,
    pub occurrence: u8,
}
impl WheelDevice {
    pub fn label(&self) -> String {
        format!(
            "{}{}",
            self.name,
            if self.occurrence == 0 {
                String::new()
            } else {
                format!(" · device {}", self.occurrence + 1)
            }
        )
    }
    fn validate(&self) -> Result<(), String> {
        if self.name.is_empty()
            || self.name.len() > 256
            || self.name.chars().count() > 64
            || self.name.chars().any(char::is_control)
            || self.occurrence >= 16
        {
            Err("Wheel device identity is invalid.".into())
        } else {
            Ok(())
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(
    tag = "type",
    content = "index",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum AxisSource {
    LeftX,
    LeftY,
    LeftZ,
    RightX,
    RightY,
    RightZ,
    LeftPedal,
    RightPedal,
    Other(u32),
}
impl AxisSource {
    fn input(self) -> GamepadInput {
        match self {
            Self::LeftX => GamepadAxis::LeftStickX.into(),
            Self::LeftY => GamepadAxis::LeftStickY.into(),
            Self::LeftZ => GamepadAxis::LeftZ.into(),
            Self::RightX => GamepadAxis::RightStickX.into(),
            Self::RightY => GamepadAxis::RightStickY.into(),
            Self::RightZ => GamepadAxis::RightZ.into(),
            Self::LeftPedal => GamepadButton::LeftTrigger2.into(),
            Self::RightPedal => GamepadButton::RightTrigger2.into(),
            Self::Other(n) => GamepadAxis::Other(n).into(),
        }
    }
    fn label(self) -> String {
        match self {
            Self::LeftX => "Left X",
            Self::LeftY => "Left Y",
            Self::LeftZ => "Left Z",
            Self::RightX => "Right X",
            Self::RightY => "Right Y",
            Self::RightZ => "Right Z",
            Self::LeftPedal => "Left trigger",
            Self::RightPedal => "Right trigger",
            Self::Other(_) => "",
        }
        .to_owned()
            + &match self {
                Self::Other(n) => format!("Other axis {n}"),
                _ => String::new(),
            }
    }
    fn validate(self) -> bool {
        !matches!(self,Self::Other(n) if n>65_535)
    }
    fn defaults() -> Vec<Self> {
        vec![
            Self::LeftX,
            Self::LeftY,
            Self::LeftZ,
            Self::RightX,
            Self::RightY,
            Self::RightZ,
            Self::LeftPedal,
            Self::RightPedal,
        ]
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AxisCalibration {
    pub source: AxisSource,
    pub minimum: f32,
    pub center: f32,
    pub maximum: f32,
    pub inverted: bool,
}
impl AxisCalibration {
    fn steering() -> Self {
        Self {
            source: AxisSource::LeftX,
            minimum: -1.,
            center: 0.,
            maximum: 1.,
            inverted: false,
        }
    }
    fn pedal(source: AxisSource) -> Self {
        Self {
            source,
            minimum: 0.,
            center: 0.5,
            maximum: 1.,
            inverted: false,
        }
    }
    fn valid(self) -> bool {
        self.source.validate()
            && [self.minimum, self.center, self.maximum]
                .iter()
                .all(|n| n.is_finite() && (-1. ..=1.).contains(n))
            && self.center - self.minimum >= 0.025
            && self.maximum - self.center >= 0.025
    }
    fn signed(self, raw: f32, deadzone: f32) -> f32 {
        if !self.valid() || !raw.is_finite() || !valid_deadzone(deadzone) {
            return 0.;
        }
        let raw = raw.clamp(self.minimum, self.maximum);
        let mut value = if raw >= self.center {
            (raw - self.center) / (self.maximum - self.center)
        } else {
            (raw - self.center) / (self.center - self.minimum)
        };
        if self.inverted {
            value = -value;
        }
        value.signum() * unipolar(value.abs(), deadzone)
    }
    fn pedal_value(self, raw: f32, deadzone: f32) -> f32 {
        if !self.valid() || !raw.is_finite() || !valid_deadzone(deadzone) {
            return 0.;
        }
        let value = ((raw - self.minimum) / (self.maximum - self.minimum)).clamp(0., 1.);
        unipolar(if self.inverted { 1. - value } else { value }, deadzone)
    }
}
fn valid_deadzone(value: f32) -> bool {
    value.is_finite() && (0. ..=0.4).contains(&value)
}
fn unipolar(value: f32, deadzone: f32) -> f32 {
    ((value - deadzone) / (1. - deadzone)).clamp(0., 1.)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WheelSettings {
    pub enabled: bool,
    pub device: Option<WheelDevice>,
    pub steering: AxisCalibration,
    pub accelerator: AxisCalibration,
    pub brake: AxisCalibration,
    pub combined_pedals: bool,
    pub deadzone: f32,
    pub steering_calibrated: bool,
    pub accelerator_calibrated: bool,
    pub brake_calibrated: bool,
}
impl Default for WheelSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            device: None,
            steering: AxisCalibration::steering(),
            accelerator: AxisCalibration::pedal(AxisSource::RightPedal),
            brake: AxisCalibration::pedal(AxisSource::LeftPedal),
            combined_pedals: false,
            deadzone: 0.05,
            steering_calibrated: false,
            accelerator_calibrated: false,
            brake_calibrated: false,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WheelChannel {
    Steering,
    Accelerator,
    Brake,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WheelAction {
    Enable,
    Device,
    Mode,
    Deadzone,
    Source(WheelChannel),
    Invert(WheelChannel),
    Calibrate(WheelChannel),
    CancelCalibration,
}
#[derive(Debug, Clone)]
struct Capture {
    channel: WheelChannel,
    device: WheelDevice,
    source: AxisSource,
    values: Vec<f32>,
}
#[derive(Debug, Clone, Default)]
pub struct WheelLive {
    pub devices: Vec<(Entity, WheelDevice)>,
    pub values: BTreeMap<AxisSource, f32>,
    capture: Option<Capture>,
    armed: bool,
}
impl WheelLive {
    pub fn observe<'a>(
        &mut self,
        devices: impl Iterator<Item = (Entity, &'a Gamepad, Option<&'a Name>)>,
        profile: &WheelSettings,
    ) -> bool {
        let previous = self.selected(profile);
        let mut sorted: Vec<_> = devices.collect();
        sorted.sort_by_key(|(entity, _, _)| entity.to_bits());
        let mut found = Vec::<(Entity, WheelDevice)>::new();
        let mut selected = None;
        for (entity, pad, name) in sorted.into_iter().take(16) {
            let name = name
                .map_or("Controller", |n| n.as_str())
                .chars()
                .filter(|c| !c.is_control())
                .take(64)
                .collect::<String>();
            let name = if name.is_empty() {
                "Controller".into()
            } else {
                name
            };
            let mut device = WheelDevice {
                name,
                vendor: pad.vendor_id(),
                product: pad.product_id(),
                occurrence: 0,
            };
            device.occurrence = found
                .iter()
                .filter(|(_, d)| {
                    d.name == device.name
                        && d.vendor == device.vendor
                        && d.product == device.product
                })
                .count() as u8;
            if profile.device.as_ref() == Some(&device) {
                selected = Some(pad);
            }
            found.push((entity, device));
        }
        let changed = self.devices != found;
        self.devices = found;
        self.values.clear();
        if let Some(pad) = selected {
            let mut sources = AxisSource::defaults();
            let mut extras: Vec<_> = pad
                .get_analog_axes()
                .filter_map(|source| match source {
                    GamepadInput::Axis(GamepadAxis::Other(n)) if *n <= 65_535 => {
                        Some(AxisSource::Other(*n))
                    }
                    _ => None,
                })
                .collect();
            extras.sort();
            extras.dedup();
            sources.extend(extras.into_iter().take(32));
            for source in sources {
                if let Some(value) = pad
                    .get_unclamped(source.input())
                    .filter(|v| v.is_finite() && (-1.001..=1.001).contains(v))
                {
                    self.values.insert(source, value.clamp(-1., 1.));
                }
            }
        } else {
            self.capture = None;
        }
        if self.selected(profile) != previous || !profile.enabled || !profile.calibrated() {
            self.armed = false;
        }
        let reading = profile.read_unarmed(self);
        if !reading.ready {
            self.armed = false;
        }
        if reading.ready
            && reading.accelerator < 0.02
            && reading.brake < 0.02
            && reading.steering.abs() < 0.2
        {
            self.armed = true;
        }
        changed
    }
    pub fn selected(&self, profile: &WheelSettings) -> Option<Entity> {
        self.devices
            .iter()
            .find(|(_, d)| profile.device.as_ref() == Some(d))
            .map(|(entity, _)| *entity)
    }
    pub fn disarm(&mut self) {
        self.armed = false;
    }
    pub fn cancel(&mut self) -> bool {
        self.capture.take().is_some()
    }
    pub fn status(&self, profile: &WheelSettings) -> String {
        if !profile.enabled {
            return "Wheel / joystick driving is disabled.".into();
        }
        if self.selected(profile).is_none() {
            return "Selected wheel unavailable · wheel pedals and steering released".into();
        }
        if !profile.calibrated() {
            let raw = |axis: AxisSource| {
                self.values
                    .get(&axis)
                    .map(|n| format!("{n:+.2}"))
                    .unwrap_or_else(|| "missing".into())
            };
            return format!(
                "Wheel connected · calibrate steering and each pedal\nRaw: steering {} · accelerator {} · brake {}",
                raw(profile.steering.source),
                raw(profile.accelerator.source),
                raw(profile.brake.source)
            );
        }
        let read = profile.read_unarmed(self);
        if !read.ready {
            return "Wheel axis missing or invalid · choose axes and calibrate before driving"
                .into();
        }
        if !self.armed {
            return format!(
                "Wheel ready · release pedals and center steering to arm driving\nLive: steer {:+.0}% · accelerator {:.0}% · brake {:.0}%",
                read.steering * 100.,
                read.accelerator * 100.,
                read.brake * 100.
            );
        }
        format!(
            "Wheel · steer {:+.0}% · accelerator {:.0}% · brake {:.0}%",
            read.steering * 100.,
            read.accelerator * 100.,
            read.brake * 100.
        )
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct WheelReading {
    pub steering: f32,
    pub accelerator: f32,
    pub brake: f32,
    pub ready: bool,
}
impl WheelSettings {
    pub fn validate(&self) -> Result<(), String> {
        if let Some(device) = &self.device {
            device.validate()?;
        }
        if !self.steering.valid()
            || !self.accelerator.valid()
            || !self.brake.valid()
            || !valid_deadzone(self.deadzone)
            || self.steering.source == self.accelerator.source
            || (!self.combined_pedals
                && (self.brake.source == self.steering.source
                    || self.brake.source == self.accelerator.source))
        {
            return Err("Wheel axes must be distinct, finite and calibrated with a usable range; deadzone must be 0–40%.".into());
        }
        Ok(())
    }
    fn channel(&self, channel: WheelChannel) -> AxisCalibration {
        match channel {
            WheelChannel::Steering => self.steering,
            WheelChannel::Accelerator => self.accelerator,
            WheelChannel::Brake => self.brake,
        }
    }
    fn channel_mut(&mut self, channel: WheelChannel) -> &mut AxisCalibration {
        match channel {
            WheelChannel::Steering => &mut self.steering,
            WheelChannel::Accelerator => &mut self.accelerator,
            WheelChannel::Brake => &mut self.brake,
        }
    }
    fn channel_name(&self, channel: WheelChannel) -> &'static str {
        match channel {
            WheelChannel::Steering => "Steering",
            WheelChannel::Accelerator if self.combined_pedals => "Combined pedals",
            WheelChannel::Accelerator => "Accelerator",
            WheelChannel::Brake => "Brake",
        }
    }
    pub fn calibrated(&self) -> bool {
        self.steering_calibrated
            && self.accelerator_calibrated
            && (self.combined_pedals || self.brake_calibrated)
    }
    fn set_calibrated(&mut self, channel: WheelChannel, value: bool) {
        match channel {
            WheelChannel::Steering => self.steering_calibrated = value,
            WheelChannel::Accelerator => self.accelerator_calibrated = value,
            WheelChannel::Brake => self.brake_calibrated = value,
        }
    }
    pub fn read(&self, live: &WheelLive) -> WheelReading {
        if !live.armed {
            return WheelReading::default();
        }
        self.read_unarmed(live)
    }
    fn read_unarmed(&self, live: &WheelLive) -> WheelReading {
        if !self.enabled
            || !self.calibrated()
            || self.validate().is_err()
            || live.selected(self).is_none()
        {
            return WheelReading::default();
        }
        let Some(steering) = live
            .values
            .get(&self.steering.source)
            .copied()
            .filter(|v| v.is_finite() && (-1. ..=1.).contains(v))
        else {
            return WheelReading::default();
        };
        let Some(accel) = live
            .values
            .get(&self.accelerator.source)
            .copied()
            .filter(|v| v.is_finite() && (-1. ..=1.).contains(v))
        else {
            return WheelReading::default();
        };
        let (accelerator, brake) = if self.combined_pedals {
            let value = self.accelerator.signed(accel, self.deadzone);
            (value.max(0.), (-value).max(0.))
        } else {
            let Some(brake) = live
                .values
                .get(&self.brake.source)
                .copied()
                .filter(|v| v.is_finite() && (-1. ..=1.).contains(v))
            else {
                return WheelReading::default();
            };
            (
                self.accelerator.pedal_value(accel, self.deadzone),
                self.brake.pedal_value(brake, self.deadzone),
            )
        };
        WheelReading {
            steering: self.steering.signed(steering, self.deadzone),
            accelerator,
            brake,
            ready: true,
        }
    }
    /// Keep explicit keyboard/pad steering and throttle as the higher-priority
    /// source. A resting or disconnected wheel cannot cancel those controls.
    pub fn merge_drive(
        &self,
        live: &WheelLive,
        drive: &mut terra_core::DriveInput,
        reverse: bool,
        handbrake: bool,
    ) {
        let sample = self.read(live);
        if !sample.ready {
            return;
        }
        if drive.steering.abs() < 0.001 {
            drive.steering = f64::from(sample.steering);
        }
        if drive.throttle.abs() < 0.001 {
            drive.throttle = f64::from(sample.accelerator) * if reverse { -1. } else { 1. };
        }
        drive.brake = drive.brake.max(f64::from(sample.brake));
        drive.handbrake |= handbrake;
    }
    pub fn handle(&mut self, live: &mut WheelLive, action: WheelAction) -> String {
        live.disarm();
        if !matches!(action, WheelAction::Calibrate(_)) {
            live.capture = None;
        }
        match action {
            WheelAction::Enable => {
                self.enabled = !self.enabled;
            }
            WheelAction::Device => {
                if live.devices.is_empty() {
                    return "No Bevy/Gilrs device detected. Connect it, then choose a device."
                        .into();
                }
                let current = live
                    .devices
                    .iter()
                    .position(|(_, d)| self.device.as_ref() == Some(d));
                let index = current.map_or(0, |i| (i + 1) % live.devices.len());
                if self.device.as_ref() == Some(&live.devices[index].1) {
                    return "This is the only matching connected device; calibration retained."
                        .into();
                }
                self.device = Some(live.devices[index].1.clone());
                live.values.clear();
                self.steering_calibrated = false;
                self.accelerator_calibrated = false;
                self.brake_calibrated = false;
            }
            WheelAction::Mode => {
                self.combined_pedals = !self.combined_pedals;
                self.accelerator_calibrated = false;
                self.brake_calibrated = false;
                self.accelerator = if self.combined_pedals {
                    AxisCalibration {
                        source: AxisSource::RightZ,
                        ..AxisCalibration::steering()
                    }
                } else {
                    AxisCalibration::pedal(AxisSource::RightPedal)
                };
                self.brake = AxisCalibration::pedal(AxisSource::LeftPedal);
                if self.steering.source == self.accelerator.source
                    || (!self.combined_pedals && self.steering.source == self.brake.source)
                {
                    self.steering = AxisCalibration::steering();
                    self.steering_calibrated = false;
                }
            }
            WheelAction::Deadzone => {
                let options = [0., 0.025, 0.05, 0.1, 0.15, 0.2, 0.3, 0.4];
                let index = options
                    .iter()
                    .position(|x| (*x - self.deadzone).abs() < 0.001)
                    .unwrap_or(options.len() - 1);
                self.deadzone = options[(index + 1) % options.len()];
            }
            WheelAction::Source(channel) => {
                let mut sources = AxisSource::defaults();
                sources.extend(
                    live.values
                        .keys()
                        .copied()
                        .filter(|source| matches!(source, AxisSource::Other(_))),
                );
                let used: Vec<_> = [
                    WheelChannel::Steering,
                    WheelChannel::Accelerator,
                    WheelChannel::Brake,
                ]
                .into_iter()
                .filter(|other| {
                    *other != channel && (!self.combined_pedals || *other != WheelChannel::Brake)
                })
                .map(|other| self.channel(other).source)
                .collect();
                sources.retain(|source| !used.contains(source));
                let current = self.channel(channel).source;
                let index = sources
                    .iter()
                    .position(|source| *source == current)
                    .map_or(0, |i| (i + 1) % sources.len());
                let signed = channel == WheelChannel::Steering
                    || (self.combined_pedals && channel == WheelChannel::Accelerator);
                self.set_calibrated(channel, false);
                *self.channel_mut(channel) = if signed {
                    AxisCalibration {
                        source: sources[index],
                        ..AxisCalibration::steering()
                    }
                } else {
                    AxisCalibration::pedal(sources[index])
                };
            }
            WheelAction::Invert(channel) => {
                let mapping = self.channel_mut(channel);
                mapping.inverted = !mapping.inverted;
            }
            WheelAction::CancelCalibration => {
                return "Wheel calibration cancelled; previous valid ranges remain active.".into();
            }
            WheelAction::Calibrate(channel) => return self.capture(live, channel),
        }
        "Wheel controls updated for this session. Review the live values; Save controls keeps this profile.".into()
    }
    fn capture(&mut self, live: &mut WheelLive, channel: WheelChannel) -> String {
        let Some(device) = self
            .device
            .clone()
            .filter(|_| live.selected(self).is_some())
        else {
            return "Choose a connected wheel before calibrating.".into();
        };
        let mapping = self.channel(channel);
        let Some(raw) = live.values.get(&mapping.source).copied() else {
            return "This axis has no finite input. Choose its correct device axis first.".into();
        };
        let signed = channel == WheelChannel::Steering
            || (self.combined_pedals && channel == WheelChannel::Accelerator);
        let same = live.capture.as_ref().is_some_and(|c| {
            c.channel == channel && c.device == device && c.source == mapping.source
        });
        if !same {
            live.capture = Some(Capture {
                channel,
                device,
                source: mapping.source,
                values: Vec::new(),
            });
        }
        let capture = live.capture.as_mut().unwrap();
        capture.values.push(raw);
        let required = if signed { 3 } else { 2 };
        if capture.values.len() < required {
            return format!(
                "Captured {}. {}",
                self.channel_name(channel),
                self.calibration_prompt(channel, capture.values.len())
            );
        }
        let values = std::mem::take(&mut live.capture).unwrap().values;
        let (minimum, center, maximum, inverted) = if signed {
            (
                values[1].min(values[2]),
                values[0],
                values[1].max(values[2]),
                values[2] < values[1],
            )
        } else {
            (
                values[0].min(values[1]),
                (values[0] + values[1]) * 0.5,
                values[0].max(values[1]),
                values[1] < values[0],
            )
        };
        let next = AxisCalibration {
            source: mapping.source,
            minimum,
            center,
            maximum,
            inverted,
        };
        if !next.valid() {
            return "Calibration range was too small or neutral was outside the endpoints. Previous ranges kept; start again at neutral/released.".into();
        }
        *self.channel_mut(channel) = next;
        self.set_calibrated(channel, true);
        format!(
            "{} calibrated. Check live values at rest and full travel; Save controls keeps it.",
            self.channel_name(channel)
        )
    }
    fn calibration_prompt(&self, channel: WheelChannel, stage: usize) -> &'static str {
        match (channel, self.combined_pedals, stage) {
            (WheelChannel::Steering, _, 0) => "Center wheel, then capture neutral",
            (WheelChannel::Steering, _, 1) => "Turn fully left, then capture",
            (WheelChannel::Steering, _, _) => "Turn fully right, then capture",
            (WheelChannel::Accelerator, true, 0) => "Release both pedals, then capture neutral",
            (WheelChannel::Accelerator, true, 1) => "Press brake fully, then capture",
            (WheelChannel::Accelerator, true, _) => "Press accelerator fully, then capture",
            (_, _, 0) => "Release pedal, then capture",
            (_, _, _) => "Press pedal fully, then capture",
        }
    }
    pub fn rows(&self, live: &WheelLive) -> Vec<(String, WheelAction)> {
        let mut rows = vec![(
            format!(
                "Wheel / joystick driving · {}",
                if self.enabled { "Enabled" } else { "Disabled" }
            ),
            WheelAction::Enable,
        )];
        if !self.enabled {
            return rows;
        }
        rows.push((
            format!(
                "Wheel device · {} / Change",
                self.device
                    .as_ref()
                    .map(|d| d.label())
                    .unwrap_or_else(|| "Choose connected device".into())
            ),
            WheelAction::Device,
        ));
        rows.push((
            format!(
                "Pedal layout · {}",
                if self.combined_pedals {
                    "Combined axis"
                } else {
                    "Separate axes"
                }
            ),
            WheelAction::Mode,
        ));
        rows.push((
            format!("Wheel / pedal deadzone · {:.1}%", self.deadzone * 100.),
            WheelAction::Deadzone,
        ));
        for channel in [
            WheelChannel::Steering,
            WheelChannel::Accelerator,
            WheelChannel::Brake,
        ] {
            if self.combined_pedals && channel == WheelChannel::Brake {
                continue;
            }
            let mapping = self.channel(channel);
            let name = self.channel_name(channel);
            rows.push((
                format!("{name} axis · {}", mapping.source.label()),
                WheelAction::Source(channel),
            ));
            rows.push((
                format!(
                    "{name} direction · {}",
                    if mapping.inverted {
                        "Inverted"
                    } else {
                        "Normal"
                    }
                ),
                WheelAction::Invert(channel),
            ));
            let stage = live
                .capture
                .as_ref()
                .filter(|c| c.channel == channel)
                .map_or(0, |c| c.values.len());
            rows.push((
                format!("{name} · {}", self.calibration_prompt(channel, stage)),
                WheelAction::Calibrate(channel),
            ));
        }
        if live.capture.is_some() {
            rows.push((
                "Cancel wheel calibration".into(),
                WheelAction::CancelCalibration,
            ));
        }
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (WheelSettings, WheelLive, Gamepad, Name, Entity) {
        let mut world = World::new();
        let entity = world.spawn_empty().id();
        let name = Name::new("Test wheel");
        let pad = Gamepad::default();
        let profile = WheelSettings {
            enabled: true,
            device: Some(WheelDevice {
                name: "Test wheel".into(),
                vendor: None,
                product: None,
                occurrence: 0,
            }),
            steering_calibrated: true,
            accelerator_calibrated: true,
            brake_calibrated: true,
            deadzone: 0.,
            ..Default::default()
        };
        let mut live = WheelLive::default();
        live.observe(std::iter::once((entity, &pad, Some(&name))), &profile);
        (profile, live, pad, name, entity)
    }
    fn update(
        live: &mut WheelLive,
        profile: &WheelSettings,
        pad: &Gamepad,
        name: &Name,
        entity: Entity,
    ) {
        live.observe(std::iter::once((entity, pad, Some(name))), profile);
    }
    #[test]
    fn wheel_calibration_measures_asymmetric_steering_and_inverted_pedal_endpoints() {
        let (mut profile, mut live, mut pad, name, entity) = fixture();
        profile.steering_calibrated = false;
        profile.accelerator_calibrated = false;
        profile.accelerator.source = AxisSource::RightZ;
        for raw in [0.1, -0.8, 0.9] {
            pad.analog_mut().set(GamepadAxis::LeftStickX, raw);
            update(&mut live, &profile, &pad, &name, entity);
            profile.handle(&mut live, WheelAction::Calibrate(WheelChannel::Steering));
        }
        assert_eq!(profile.steering.minimum, -0.8);
        assert_eq!(profile.steering.center, 0.1);
        assert_eq!(profile.steering.maximum, 0.9);
        for raw in [1., -1.] {
            pad.analog_mut().set(GamepadAxis::RightZ, raw);
            update(&mut live, &profile, &pad, &name, entity);
            profile.handle(&mut live, WheelAction::Calibrate(WheelChannel::Accelerator));
        }
        assert!(profile.accelerator.inverted && profile.calibrated());
        assert_eq!(
            profile.read(&live),
            WheelReading::default(),
            "full pedal at calibration end must not arm drive"
        );
        pad.analog_mut().set(GamepadAxis::LeftStickX, 0.1);
        pad.analog_mut().set(GamepadAxis::RightZ, 1.);
        update(&mut live, &profile, &pad, &name, entity);
        assert!(profile.read(&live).ready);
        assert_eq!(profile.read(&live).accelerator, 0.);
        pad.analog_mut().set(GamepadAxis::LeftStickX, -0.8);
        pad.analog_mut().set(GamepadAxis::RightZ, -1.);
        update(&mut live, &profile, &pad, &name, entity);
        let reading = profile.read(&live);
        assert_eq!(reading.steering, -1.);
        assert_eq!(reading.accelerator, 1.);
        pad.analog_mut().set(GamepadAxis::LeftStickX, 0.9);
        pad.analog_mut().set(GamepadAxis::RightZ, 0.);
        update(&mut live, &profile, &pad, &name, entity);
        assert_eq!(profile.read(&live).steering, 1.);
        assert!((profile.read(&live).accelerator - 0.5).abs() < 1e-6);
    }
    #[test]
    fn combined_pedal_axis_calibrates_neutral_brake_accelerator_and_splits_exclusively() {
        let (mut profile, mut live, mut pad, name, entity) = fixture();
        profile.handle(&mut live, WheelAction::Mode);
        for raw in [0.2, 1., -0.8] {
            pad.analog_mut().set(GamepadAxis::RightZ, raw);
            update(&mut live, &profile, &pad, &name, entity);
            profile.handle(&mut live, WheelAction::Calibrate(WheelChannel::Accelerator));
        }
        assert!(profile.combined_pedals && profile.accelerator.inverted && profile.calibrated());
        pad.analog_mut().set(GamepadAxis::RightZ, 0.2);
        update(&mut live, &profile, &pad, &name, entity);
        assert!(profile.read(&live).ready);
        pad.analog_mut().set(GamepadAxis::RightZ, 1.);
        update(&mut live, &profile, &pad, &name, entity);
        assert_eq!(profile.read(&live).brake, 1.);
        assert_eq!(profile.read(&live).accelerator, 0.);
        pad.analog_mut().set(GamepadAxis::RightZ, -0.8);
        update(&mut live, &profile, &pad, &name, entity);
        assert_eq!(profile.read(&live).accelerator, 1.);
        assert_eq!(profile.read(&live).brake, 0.);
        profile.deadzone = 0.1;
        pad.analog_mut().set(GamepadAxis::RightZ, 0.19);
        update(&mut live, &profile, &pad, &name, entity);
        assert_eq!(profile.read(&live).accelerator, 0.);
    }
    #[test]
    fn missing_nonfinite_disconnected_and_reconnected_axes_release_and_require_neutral_again() {
        let (profile, mut live, mut pad, name, entity) = fixture();
        pad.analog_mut().set(GamepadButton::RightTrigger2, 0.8);
        update(&mut live, &profile, &pad, &name, entity);
        assert!(profile.read(&live).accelerator > 0.7);
        pad.analog_mut().set(GamepadAxis::LeftStickX, f32::NAN);
        update(&mut live, &profile, &pad, &name, entity);
        assert_eq!(profile.read(&live), WheelReading::default());
        assert!(live.status(&profile).contains("axis missing or invalid"));
        pad.analog_mut().set(GamepadAxis::LeftStickX, 0.);
        update(&mut live, &profile, &pad, &name, entity);
        assert_eq!(profile.read(&live), WheelReading::default());
        pad.analog_mut().set(GamepadButton::RightTrigger2, 0.);
        update(&mut live, &profile, &pad, &name, entity);
        assert!(profile.read(&live).ready);
        live.observe(std::iter::empty(), &profile);
        assert_eq!(profile.read(&live), WheelReading::default());
        pad.analog_mut().set(GamepadButton::RightTrigger2, 1.);
        update(&mut live, &profile, &pad, &name, entity);
        assert_eq!(profile.read(&live), WheelReading::default());
        pad.analog_mut().set(GamepadButton::RightTrigger2, 0.);
        update(&mut live, &profile, &pad, &name, entity);
        assert!(profile.read(&live).ready);
        for raw in [f32::INFINITY, f32::NEG_INFINITY, 2., -2.] {
            pad.analog_mut().set(GamepadButton::RightTrigger2, raw);
            update(&mut live, &profile, &pad, &name, entity);
            assert_eq!(profile.read(&live), WheelReading::default());
        }
    }
    #[test]
    fn invalid_or_cancelled_calibration_preserves_valid_profile_and_settings_bounds() {
        let (mut profile, mut live, _, _, _) = fixture();
        let previous = profile.steering;
        for _ in 0..3 {
            profile.handle(&mut live, WheelAction::Calibrate(WheelChannel::Steering));
        }
        assert_eq!(profile.steering, previous);
        profile.handle(&mut live, WheelAction::Calibrate(WheelChannel::Steering));
        assert!(live.capture.is_some());
        profile.handle(&mut live, WheelAction::CancelCalibration);
        assert!(live.capture.is_none());
        assert_eq!(profile.steering, previous);
        for scenario in 0..6 {
            let mut bad = profile.clone();
            match scenario {
                0 => bad.deadzone = f32::NAN,
                1 => bad.deadzone = 0.41,
                2 => bad.steering.center = bad.steering.maximum,
                3 => bad.accelerator.source = bad.steering.source,
                4 => bad.brake.source = AxisSource::Other(65_536),
                _ => bad.device.as_mut().unwrap().name = "x".repeat(65),
            };
            assert!(bad.validate().is_err());
        }
    }
    #[test]
    fn profile_handoff_preserves_bounded_calibration_but_never_devices_or_pending_capture() {
        let (profile, live, _, _, _) = fixture();
        let mut settings = crate::player_input::InputSettings::default();
        settings.wheel = profile;
        settings.wheel_live = live;
        settings.wheel.device.as_mut().unwrap().name = "界".repeat(64);
        settings.wheel.device.as_mut().unwrap().occurrence = 15;
        settings.wheel.steering.source = AxisSource::Other(65535);
        let payload = settings.to_handoff().unwrap();
        assert!(payload.len() < 4096);
        assert!(!payload.contains("wheel_live") && !payload.contains("armed"));
        let restored = crate::player_input::InputSettings::from_handoff(&payload).unwrap();
        assert_eq!(restored.wheel, settings.wheel);
        assert!(restored.wheel_live.devices.is_empty());
        assert_eq!(
            restored.wheel.read(&restored.wheel_live),
            WheelReading::default()
        );
        let legacy = crate::player_input::InputSettings::from_handoff("{}").unwrap();
        assert!(!legacy.wheel.enabled);
        legacy.validate().unwrap();
    }
    #[test]
    fn dedicated_wheel_does_not_override_keyboard_and_other_axis_selection_is_bounded() {
        let (mut profile, mut live, mut pad, name, entity) = fixture();
        pad.analog_mut().set(GamepadButton::RightTrigger2, 0.8);
        pad.analog_mut().set(GamepadAxis::LeftStickX, 0.6);
        for n in 0..80 {
            pad.analog_mut().set(GamepadAxis::Other(n), 0.);
        }
        update(&mut live, &profile, &pad, &name, entity);
        assert!(live.values.len() <= 40);
        let mut drive = terra_core::DriveInput {
            throttle: -1.,
            steering: -1.,
            brake: 0.9,
            handbrake: false,
        };
        profile.merge_drive(&live, &mut drive, false, false);
        assert_eq!(drive.throttle, -1.);
        assert_eq!(drive.steering, -1.);
        assert_eq!(drive.brake, 0.9);
        for _ in 0..60 {
            profile.handle(&mut live, WheelAction::Source(WheelChannel::Steering));
            profile.validate().unwrap();
        }
        assert!(!profile.steering_calibrated);
        assert_eq!(profile.read(&live), WheelReading::default());
    }
}
