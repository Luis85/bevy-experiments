//! Island Village sound: ambience layers cross-faded from what the view
//! shows (the shore, the woods and the wind, rain and storm, birds by day and
//! crickets by night, a village murmur, a fire close by), work one-shots from
//! the poses and machines already on screen, quiet music with long gaps and
//! a jingle on a new morning or a finished building, and the interface's
//! sounds ([`PlayUiSound`]).
//!
//! Presentation only: nothing here writes simulation state; sounds come
//! only from residents, animals and buildings the fog shows live. Files come
//! from the manifest's `living/audio/` ([`catalog`]); a missing ambience
//! loop is made in code ([`synth`]), any other missing sound is silence.
//! The decisions are pure functions ([`mix`]), tested without a device.
//! Without Bevy's audio plugin (headless hosts, tests) or with
//! `ISLAND_AUDIO_OFF=1` nothing is loaded or played.
//!
//! Every started layer and one-shot logs one `island_audio:` line, so a
//! capture can count them.
pub(crate) mod catalog;
pub(crate) mod mix;
pub(crate) mod synth;

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use bevy::{
    audio::{AddAudioSource, AudioSinkPlayback, SpatialScale, Volume},
    prelude::*,
};
use npc_framework::crafting::Crafting;
use npc_framework::prelude::{SimClock, SimConfig, SimControl, SimId, SimIndex};
use npc_framework::view_helpers::{ViewSystems, VisualIndex, VisualOf};
use testbed_content::village::{VillagePlace, frontier::terrain::TerrainState};

use crate::anim::ActorAnimTarget;
use crate::fog::{Fogged, Look};
use crate::island_atmosphere::daylight::lamp_level_at;
use crate::{FrontendGate, RtsCamera};
use mix::{Layer, Scene, Shot, Tune};

/// The switch that keeps a run silent and loads nothing (captures, tests).
pub const AUDIO_OFF_ENV: &str = "ISLAND_AUDIO_OFF";

/// The player's volumes, each 0 to 1: everything (`master`) and each kind.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct AudioLevels {
    pub master: f32,
    pub ambience: f32,
    pub effects: f32,
    pub music: f32,
    pub ui: f32,
}

impl Default for AudioLevels {
    fn default() -> Self {
        Self {
            master: 0.8,
            ambience: 0.7,
            effects: 0.7,
            music: 0.7,
            ui: 0.7,
        }
    }
}

impl AudioLevels {
    fn bus(&self, level: f32) -> f32 {
        (self.master * level).clamp(0.0, 1.0)
    }
    pub fn ambience_gain(&self) -> f32 {
        self.bus(self.ambience)
    }
    pub fn effects_gain(&self) -> f32 {
        self.bus(self.effects)
    }
    pub fn music_gain(&self) -> f32 {
        self.bus(self.music)
    }
    pub fn ui_gain(&self) -> f32 {
        self.bus(self.ui)
    }
}

/// The interface's sounds, played by [`PlayUiSound`].
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub enum UiSound {
    Click,
    Hover,
    Open,
    Close,
    Confirm,
    Deny,
    Place,
    Alert,
    Coin,
    Page,
}

impl UiSound {
    pub(crate) const ALL: [UiSound; 10] = [
        UiSound::Click,
        UiSound::Hover,
        UiSound::Open,
        UiSound::Close,
        UiSound::Confirm,
        UiSound::Deny,
        UiSound::Place,
        UiSound::Alert,
        UiSound::Coin,
        UiSound::Page,
    ];

    /// Words a recorded file's name is matched by, best first.
    pub(crate) fn words(self) -> &'static [&'static str] {
        match self {
            UiSound::Click => &["click", "tap", "select"],
            UiSound::Hover => &["hover", "rollover", "tick"],
            UiSound::Open => &["open", "maximize", "drop"],
            UiSound::Close => &["close", "minimize"],
            UiSound::Confirm => &["confirm", "accept", "ok", "success"],
            UiSound::Deny => &["deny", "error", "cancel", "back"],
            UiSound::Place => &["place", "build", "put", "thud"],
            UiSound::Alert => &["alert", "warning", "bell", "notify"],
            UiSound::Coin => &["coin", "money", "cash"],
            UiSound::Page => &["page", "book", "paper", "flip"],
        }
    }
}

/// Ask for an interface sound (the UI writes these).
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayUiSound(pub UiSound);

/// The host's older volume state (the frontend's settings write it; `--mute`
/// mutes it). Its changes are copied into [`AudioLevels`]: master, ambience
/// and effects, and a mute as a master of 0.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct IslandAudioSettings {
    pub master: f32,
    pub ambience: f32,
    pub effects: f32,
    pub muted: bool,
}

impl Default for IslandAudioSettings {
    fn default() -> Self {
        Self {
            master: 0.6,
            ambience: 0.45,
            effects: 0.55,
            muted: false,
        }
    }
}

impl IslandAudioSettings {
    pub fn muted() -> Self {
        Self {
            muted: true,
            ..Self::default()
        }
    }
}

/// The levels after a change of the older settings.
pub(crate) fn levels_from(settings: IslandAudioSettings, levels: AudioLevels) -> AudioLevels {
    AudioLevels {
        master: if settings.muted { 0.0 } else { settings.master },
        ambience: settings.ambience,
        effects: settings.effects,
        ..levels
    }
}

/// The files found at startup, by sound, with each one's own gain.
#[derive(Resource, Default)]
struct Sounds {
    shots: BTreeMap<Shot, Vec<(Handle<AudioSource>, f32)>>,
    ui: BTreeMap<UiSound, (Handle<AudioSource>, f32)>,
    music: BTreeMap<u8, (Handle<AudioSource>, f32)>,
}

/// An ambience layer's player: one, or the second copy of a twinned layer.
#[derive(Component)]
struct LayerVoice {
    layer: Layer,
    /// 0 the first copy, 1 the twin that starts half a loop later.
    copy: u8,
    level: f32,
    /// The sound's own gain ([`mix::sound_gain`]).
    gain: f32,
    /// Where its sound comes from, for the log: a file or a made loop.
    source: String,
    audible: bool,
}

/// Seconds the twin of a short loop starts after the first copy: half the
/// recording's length (`wind` 6.6 s, `fire` 3.5 s).
fn twin_delay(layer: Layer) -> f32 {
    match layer {
        Layer::Wind => 3.3,
        Layer::Fire => 1.8,
        _ => 0.0,
    }
}

/// A one-shot sounding now (counted against [`mix::MAX_VOICES`]).
#[derive(Component)]
struct ShotVoice;

#[derive(Component)]
struct IslandListener;

/// The slower part of the scene's reading (every half second): what is
/// round the camera's focus.
#[derive(Resource, Default)]
struct Surroundings {
    since: f32,
    coast_m: f32,
    forest: f32,
    residents: usize,
    fire: f32,
}

/// Per-emitter cadence and the one-second window of one-shots started.
#[derive(Resource, Default)]
pub(crate) struct ShotLimiter {
    next: BTreeMap<(Shot, u32), f32>,
    recent: VecDeque<f32>,
    count: BTreeMap<u32, u32>,
}

impl ShotLimiter {
    /// Whether `emitter` may sound `shot` at `now` with `per_second`
    /// one-shots allowed and its cadence stretched by `wait`. A yes is
    /// recorded as played.
    pub(crate) fn try_emit(
        &mut self,
        shot: Shot,
        emitter: u32,
        now: f32,
        per_second: usize,
        wait: f32,
    ) -> bool {
        while self.recent.front().is_some_and(|at| now - *at >= 1.0) {
            self.recent.pop_front();
        }
        if self
            .next
            .get(&(shot, emitter))
            .is_some_and(|next| now < *next)
        {
            return false;
        }
        if self.recent.len() >= per_second {
            return false;
        }
        let swing = 0.9 + (emitter.wrapping_mul(2_654_435_761) >> 24) as f32 / 255.0 * 0.25;
        self.next
            .insert((shot, emitter), now + shot.interval() * swing * wait);
        self.recent.push_back(now);
        *self.count.entry(emitter).or_default() += 1;
        true
    }

    fn played(&self, emitter: u32) -> u32 {
        self.count.get(&emitter).copied().unwrap_or(0)
    }

    pub(crate) fn forget_idle(&mut self, now: f32) {
        self.next.retain(|_, next| now - *next < 20.0);
    }
}

/// The music's state: what plays, when the next track may start, the day
/// and buildings last seen (for the jingles).
#[derive(Resource, Default)]
struct Music {
    playing: Option<(Entity, f32)>,
    next_at: f32,
    tracks: u32,
    day: Option<u64>,
    buildings: Option<usize>,
}

/// When the next thunder of a storm comes, and how many there have been.
#[derive(Resource, Default)]
struct Thunder {
    next_at: f32,
    count: u32,
}

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<IslandAudioSettings>()
        .init_resource::<AudioLevels>();
    if !app.world().contains_resource::<Messages<PlayUiSound>>() {
        app.add_message::<PlayUiSound>();
    }
    if std::env::var(AUDIO_OFF_ENV).is_ok_and(|value| !value.is_empty() && value != "0") {
        return;
    }
    // Only with Bevy's audio plugin (not in headless hosts and tests).
    if !app.world().contains_resource::<Assets<AudioSource>>() {
        return;
    }
    app.add_audio_source::<synth::Synth>()
        .init_resource::<ShotLimiter>()
        .init_resource::<Surroundings>()
        .init_resource::<Music>()
        .init_resource::<Thunder>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                sync_levels,
                follow_listener,
                survey,
                ambience,
                thunder,
                work_sounds,
                music,
                ui_sounds,
            )
                .chain()
                .in_set(ViewSystems::Present),
        );
}

fn sync_levels(settings: Res<IslandAudioSettings>, mut levels: ResMut<AudioLevels>) {
    if settings.is_changed() {
        let next = levels_from(*settings, *levels);
        if *levels != next {
            *levels = next;
        }
    }
}

/// The stem of a manifest path (`living/audio/work/chop_1.ogg` → `chop_1`).
fn stem_of(path: &str) -> &str {
    std::path::Path::new(path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(path)
}

/// Load what the manifest lists and start every layer silent.
fn setup(mut commands: Commands, server: Res<AssetServer>, mut made: ResMut<Assets<synth::Synth>>) {
    let root = &crate::assets::asset_dir().path;
    let paths = catalog::available(root);
    info!(
        "island_audio: {} sound files found under {}",
        paths.len(),
        root.display()
    );
    let load = |path: &str| -> (Handle<AudioSource>, f32) {
        (
            server.load(path.to_string()),
            mix::sound_gain(stem_of(path)),
        )
    };
    let mut sounds = Sounds::default();
    for shot in Shot::ALL {
        let files = shot
            .stems()
            .iter()
            .filter_map(|stem| catalog::stem(&paths, shot.folder(), stem))
            .map(load)
            .collect();
        sounds.shots.insert(shot, files);
    }
    for sound in UiSound::ALL {
        if let Some(path) = catalog::pick(&paths, "ui", sound.words()) {
            sounds.ui.insert(sound, load(path));
        }
    }
    for tune in [Tune::Day, Tune::Night, Tune::Morning, Tune::Built] {
        if let Some(path) = catalog::stem(&paths, "music", tune.stem()) {
            sounds.music.insert(tune as u8, load(path));
        }
    }
    commands.insert_resource(sounds);
    for layer in Layer::ALL {
        let file = layer
            .file()
            .and_then(|stem| catalog::stem(&paths, "ambience", stem));
        let made_kind = file.is_none().then(|| layer.made()).flatten();
        let (source, gain) = match (file, made_kind) {
            (Some(path), _) => (path.to_string(), mix::sound_gain(stem_of(path))),
            (None, Some(kind)) => (kind.name().to_string(), mix::sound_gain(kind.name())),
            (None, None) => continue,
        };
        // A made loop is generated once and shared by the copies.
        let made_handle = made_kind.map(|kind| {
            made.add(synth::Synth {
                samples: synth::generate(kind).into(),
            })
        });
        let copies = if layer.twinned() { 2 } else { 1 };
        for copy in 0..copies {
            let voice = commands
                .spawn((
                    Name::new(format!("ambience {} {copy}", layer.name())),
                    LayerVoice {
                        layer,
                        copy,
                        level: 0.0,
                        gain: if copies == 2 { gain * 0.6 } else { gain },
                        source: source.clone(),
                        audible: false,
                    },
                    PlaybackSettings::LOOP.with_volume(Volume::SILENT).paused(),
                ))
                .id();
            match (file, &made_handle) {
                (Some(path), _) => {
                    commands
                        .entity(voice)
                        .insert(AudioPlayer::new(server.load(path.to_string())));
                }
                (None, Some(handle)) => {
                    commands
                        .entity(voice)
                        .insert(AudioPlayer::<synth::Synth>(handle.clone()));
                }
                (None, None) => {}
            }
        }
    }
    commands.spawn((
        Name::new("island listener"),
        IslandListener,
        SpatialListener::new(6.0),
        Transform::default(),
    ));
}

/// Ears at the point the camera looks at, turned with the view, so work
/// near the middle of the screen is loudest and pans as seen.
fn follow_listener(
    cameras: Query<&RtsCamera>,
    mut listeners: Query<&mut Transform, With<IslandListener>>,
) {
    let Some(camera) = cameras.iter().next() else {
        return;
    };
    for mut transform in &mut listeners {
        let want = Transform::from_translation(camera.focus)
            .with_rotation(Quat::from_rotation_y(camera.yaw));
        if *transform != want {
            *transform = want;
        }
    }
}

/// Every half second: how far the coast is, how wooded it is, how many
/// residents and how near a fire, round the camera's focus.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn survey(
    time: Res<Time<Real>>,
    mut around: ResMut<Surroundings>,
    terrain: Option<Res<TerrainState>>,
    cameras: Query<&RtsCamera>,
    trees: Query<&GlobalTransform, With<crate::island_homes::Canopy>>,
    residents: Query<&GlobalTransform, With<crate::visuals::ActorVisual>>,
    chimneys: Query<(&crate::island_atmosphere::smoke::Chimney, &GlobalTransform)>,
    forges: Query<
        (&InheritedVisibility, &GlobalTransform),
        With<crate::island_atmosphere::smoke::Forge>,
    >,
) {
    around.since += time.delta_secs();
    if around.since < 0.5 {
        return;
    }
    around.since = 0.0;
    let Some(camera) = cameras.iter().next() else {
        return;
    };
    let focus = Vec2::new(camera.focus.x, camera.focus.z);
    let flat = |at: &GlobalTransform| Vec2::new(at.translation().x, at.translation().z);
    around.coast_m = terrain.as_deref().map_or(200.0, |terrain| {
        mix_coast(focus, |x, z| {
            terrain.cell(x, z).is_none_or(|cell| {
                cell.surface == testbed_content::village::frontier::terrain::TerrainSurface::Water
            })
        })
    });
    let near_trees = trees
        .iter()
        .filter(|at| flat(at).distance_squared(focus) < 25.0 * 25.0)
        .count();
    around.forest = (near_trees as f32 / 40.0).min(1.0);
    around.residents = residents
        .iter()
        .filter(|at| flat(at).distance_squared(focus) < 25.0 * 25.0)
        .count();
    let fire_at = |distance: f32| (1.0 - distance / 18.0).clamp(0.0, 1.0);
    let hearth = chimneys
        .iter()
        .map(|(chimney, at)| chimney.level() * fire_at(flat(at).distance(focus)))
        .fold(0.0, f32::max);
    let forge = forges
        .iter()
        .filter(|(shown, _)| shown.get())
        .map(|(_, at)| fire_at(flat(at).distance(focus)))
        .fold(0.0, f32::max);
    around.fire = hearth.max(forge);
}

/// Metres from `focus` to the nearest water cell within 60 m (a coarse ring
/// search every 3 m), 200 when none.
pub(crate) fn mix_coast(focus: Vec2, water: impl Fn(i32, i32) -> bool) -> f32 {
    let mut best: f32 = 200.0;
    for dz in (-60..=60).step_by(3) {
        for dx in (-60..=60).step_by(3) {
            let distance = ((dx * dx + dz * dz) as f32).sqrt();
            if distance >= best || distance > 60.0 {
                continue;
            }
            let (x, z) = (
                (focus.x + dx as f32).floor() as i32,
                (focus.y + dz as f32).floor() as i32,
            );
            if water(x, z) {
                best = distance;
            }
        }
    }
    best
}

/// The storm's weight now, 0 .. 1, from the weather the view draws.
fn storm_of(look: Option<&crate::island_world::weather::WeatherLook>) -> (f32, f32) {
    look.map_or((0.0, 0.0), |look| {
        use testbed_content::village::frontier::life_spec::Weather;
        match look.kind {
            Weather::Rain => (look.amount, 0.0),
            Weather::Storm => (look.amount, look.amount),
            _ => (0.0, 0.0),
        }
    })
}

#[allow(clippy::too_many_arguments)]
fn ambience(
    time: Res<Time<Real>>,
    levels: Res<AudioLevels>,
    around: Res<Surroundings>,
    (clock, config, control, gate): (
        Option<Res<SimClock>>,
        Option<Res<SimConfig>>,
        Option<Res<SimControl>>,
        Option<Res<FrontendGate>>,
    ),
    (weather, wind): (
        Option<Res<crate::island_world::weather::WeatherLook>>,
        Option<Res<crate::island_world::ambient::Wind>>,
    ),
    cameras: Query<&RtsCamera>,
    mut voices: Query<(&mut LayerVoice, Option<&mut AudioSink>)>,
) {
    let night = clock
        .as_deref()
        .zip(config.as_deref())
        .map_or(0.0, |(clock, config)| lamp_level_at(clock.tick, config));
    let (rain, storm) = storm_of(weather.as_deref());
    let held =
        control.as_deref().is_none_or(SimControl::is_stopped) || gate.is_some_and(|gate| gate.0);
    let scene = Scene {
        coast_m: around.coast_m,
        forest: around.forest,
        wind: wind.as_deref().map_or(1.0, |wind| wind.strength),
        rain,
        storm,
        night,
        residents: around.residents,
        fire: around.fire,
        camera_m: cameras.iter().next().map_or(60.0, |camera| camera.distance),
        held,
    };
    let gains = mix::layer_gains(scene);
    let bus = levels.ambience_gain();
    // How far each layer's first copy has played, for its twin's start.
    let played: BTreeMap<Layer, f32> = voices
        .iter()
        .filter(|(voice, _)| voice.copy == 0)
        .filter_map(|(voice, sink)| Some((voice.layer, sink?.position().as_secs_f32())))
        .collect();
    for (mut voice, sink) in &mut voices {
        let index = Layer::ALL
            .iter()
            .position(|layer| *layer == voice.layer)
            .unwrap_or(0);
        let level = mix::fade_towards(voice.level, gains[index], time.delta_secs(), 2.5);
        if voice.level != level {
            voice.level = level;
        }
        let volume = level * voice.gain * bus;
        // A twin waits until its first copy is half a loop in.
        let waiting = voice.copy == 1
            && played
                .get(&voice.layer)
                .is_none_or(|at| *at < twin_delay(voice.layer));
        let audible = volume > 0.002 && !waiting;
        if audible && !voice.audible && voice.copy == 0 {
            info!(
                "island_audio: layer {} started ({})",
                voice.layer.name(),
                voice.source
            );
        }
        if voice.audible != audible {
            voice.audible = audible;
        }
        let Some(mut sink) = sink else { continue };
        if audible {
            sink.set_volume(Volume::Linear(volume));
            if sink.is_paused() {
                sink.play();
            }
        } else if !sink.is_paused() {
            sink.pause();
        }
    }
}

/// Thunder over the rain in a storm, at uneven gaps of 12 to 40 s; on the
/// ambience's volume, quieter from a high camera.
fn thunder(
    mut commands: Commands,
    time: Res<Time<Real>>,
    levels: Res<AudioLevels>,
    sounds: Option<Res<Sounds>>,
    weather: Option<Res<crate::island_world::weather::WeatherLook>>,
    gate: Option<Res<FrontendGate>>,
    mut state: ResMut<Thunder>,
) {
    let now = time.elapsed_secs();
    let (_, storm) = storm_of(weather.as_deref());
    if storm < 0.3 || gate.is_some_and(|gate| gate.0) {
        // Not before a gap once a storm comes.
        state.next_at = state.next_at.max(now + 6.0);
        return;
    }
    if now < state.next_at {
        return;
    }
    state.count = state.count.wrapping_add(1);
    state.next_at = now + mix::thunder_gap(state.count);
    let Some((file, gain)) = sounds
        .as_deref()
        .and_then(|sounds| sounds.shots.get(&Shot::Thunder)?.first().cloned())
    else {
        return;
    };
    let volume = gain * storm * levels.ambience_gain();
    if volume <= 0.0 {
        return;
    }
    info!("island_audio: one-shot thunder");
    commands.spawn((
        Name::new("thunder"),
        ShotVoice,
        AudioPlayer::new(file),
        PlaybackSettings::DESPAWN
            .with_volume(Volume::Linear(volume))
            .with_speed(mix::pitch(state.count, 0)),
    ));
}

/// One-shots of work and movement near the camera: from the poses played,
/// the machines worked, galloping deer and doors opening. None while the
/// clock is stopped or a menu is open; a fast clock thins them.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn work_sounds(
    mut commands: Commands,
    time: Res<Time<Real>>,
    levels: Res<AudioLevels>,
    sounds: Option<Res<Sounds>>,
    (gate, control): (Option<Res<FrontendGate>>, Option<Res<SimControl>>),
    (index, visuals, doors): (
        Option<Res<SimIndex>>,
        Option<Res<VisualIndex>>,
        Option<Res<crate::island_homes::OpenDoors>>,
    ),
    mut limiter: ResMut<ShotLimiter>,
    mut seen_doors: Local<BTreeSet<Entity>>,
    cameras: Query<&RtsCamera>,
    players: Query<&ActorAnimTarget>,
    roots: Query<(
        &VisualOf,
        &GlobalTransform,
        Option<&Fogged>,
        &InheritedVisibility,
    )>,
    (ids, crafts, places): (Query<&SimId>, Query<&Crafting>, Query<&VillagePlace>),
    deer: Query<(&crate::island_world::wildlife::DeerPlayer, &GlobalTransform)>,
    placed: Query<&GlobalTransform>,
    voices: Query<(), With<ShotVoice>>,
) {
    let Some(sounds) = sounds else { return };
    let gain = levels.effects_gain();
    let stopped = control.as_deref().is_none_or(SimControl::is_stopped);
    // Doors opened this frame are remembered even while quiet, so a pause
    // does not leave a burst of them for later.
    let open: BTreeSet<Entity> = doors
        .as_deref()
        .map_or_else(BTreeSet::new, |doors| doors.leaves().collect());
    let opened: Vec<Entity> = open.difference(&seen_doors).copied().collect();
    *seen_doors = open;
    if gain <= 0.0 || stopped || gate.is_some_and(|gate| gate.0) {
        return;
    }
    let Some(camera) = cameras.iter().next() else {
        return;
    };
    let now = time.elapsed_secs();
    let (per_second, wait) = mix::thinning(control.as_deref().map_or(1, |c| c.multiplier));
    let audible = |at: Vec3| at.distance(camera.focus) <= mix::AUDIBLE_M;
    let heard = |root: Entity| -> Option<(u32, Vec3)> {
        let (of, at, fog, visible) = roots.get(root).ok()?;
        // Never give away work the player cannot currently see.
        if !visible.get() || fog.is_some_and(|fog| fog.0 != Look::Live) {
            return None;
        }
        let at = at.translation();
        audible(at).then_some(())?;
        Some((ids.get(of.0).ok()?.0, at))
    };
    let mut emitters: Vec<(f32, Shot, u32, Vec3)> = Vec::new();
    for player in &players {
        let Some(shot) = player.playing().and_then(mix::pose_shot) else {
            continue;
        };
        if let Some((id, at)) = heard(player.root()) {
            emitters.push((at.distance_squared(camera.focus), shot, id, at));
        }
    }
    if let (Some(index), Some(visuals)) = (index.as_deref(), visuals.as_deref()) {
        for craft in &crafts {
            let Some(place) = craft.workstation.and_then(|station| index.get(station)) else {
                continue;
            };
            let Some(shot) = places
                .get(place)
                .ok()
                .and_then(|place| mix::machine_shot(place.role))
            else {
                continue;
            };
            if let Some((id, at)) = visuals.map.get(&place).and_then(|root| heard(*root)) {
                emitters.push((at.distance_squared(camera.focus), shot, id, at));
            }
        }
    }
    for (player, at) in &deer {
        let at = at.translation();
        if player.galloping() && audible(at) {
            let id = (at.x * 10.0) as i32 as u32 ^ ((at.z * 10.0) as i32 as u32).rotate_left(16);
            emitters.push((at.distance_squared(camera.focus), Shot::Gallop, id, at));
        }
    }
    for door in opened {
        if let Ok(at) = placed.get(door)
            && audible(at.translation())
        {
            let at = at.translation();
            emitters.push((
                at.distance_squared(camera.focus),
                Shot::Door,
                door.to_bits() as u32,
                at,
            ));
        }
    }
    // Nearest first, so the caps keep what is closest to the view.
    emitters.sort_by(|a, b| a.0.total_cmp(&b.0).then((a.1, a.2).cmp(&(b.1, b.2))));
    emitters.dedup_by_key(|emitter| (emitter.1, emitter.2));
    let mut voices_now = voices.iter().count();
    let volume = gain * mix::nearness(camera.distance);
    for (_, shot, id, at) in emitters {
        if voices_now >= mix::MAX_VOICES {
            break;
        }
        let Some(files) = sounds.shots.get(&shot).filter(|files| !files.is_empty()) else {
            continue;
        };
        if !limiter.try_emit(shot, id, now, per_second, wait) {
            continue;
        }
        let count = limiter.played(id);
        let (source, own) = files[count as usize % files.len()].clone();
        info!(
            "island_audio: one-shot {} at {:.1},{:.1}",
            shot.name(),
            at.x,
            at.z
        );
        commands.spawn((
            Name::new("island one-shot"),
            ShotVoice,
            AudioPlayer::new(source),
            PlaybackSettings::DESPAWN
                .with_spatial(true)
                .with_spatial_scale(SpatialScale::new(1.0 / 14.0))
                .with_volume(Volume::Linear(volume * own))
                .with_speed(mix::pitch(id, count)),
            Transform::from_translation(at),
        ));
        voices_now += 1;
    }
    limiter.forget_idle(now);
}

/// Quiet day and night tracks with long gaps between them, a jingle on a
/// new morning and on a finished building; held while a menu is open.
#[allow(clippy::too_many_arguments)]
fn music(
    mut commands: Commands,
    time: Res<Time<Real>>,
    levels: Res<AudioLevels>,
    sounds: Option<Res<Sounds>>,
    mut state: ResMut<Music>,
    (clock, config, gate): (
        Option<Res<SimClock>>,
        Option<Res<SimConfig>>,
        Option<Res<FrontendGate>>,
    ),
    buildings: Query<(), With<testbed_content::village::civic::Building>>,
    mut sinks: Query<&mut AudioSink>,
) {
    let Some(sounds) = sounds else { return };
    let now = time.elapsed_secs();
    let menu = gate.is_some_and(|gate| gate.0);
    // A track that ended has despawned itself.
    if let Some((playing, gain)) = state.playing {
        match sinks.get_mut(playing) {
            Ok(mut sink) => {
                if menu && !sink.is_paused() {
                    sink.pause();
                } else if !menu && sink.is_paused() {
                    sink.play();
                }
                sink.set_volume(Volume::Linear(levels.music_gain() * gain));
            }
            Err(_) if commands.get_entity(playing).is_err() => state.playing = None,
            Err(_) => {}
        }
    }
    let (Some(clock), Some(config)) = (clock, config) else {
        return;
    };
    let day = clock.tick / config.day_length.max(1);
    let built = buildings.iter().count();
    let jingle = if state.day.is_some_and(|seen| day > seen) {
        Some(Tune::Morning)
    } else if state.buildings.is_some_and(|seen| built > seen) && now > 5.0 {
        Some(Tune::Built)
    } else {
        None
    };
    state.day = Some(day);
    state.buildings = Some(built);
    if menu || levels.music_gain() <= 0.0 {
        return;
    }
    let tune = match jingle {
        Some(tune) => tune,
        None if state.playing.is_some() || now < state.next_at => return,
        None => mix::tune_for(lamp_level_at(clock.tick, &config)),
    };
    let Some((track, gain)) = sounds.music.get(&(tune as u8)).cloned() else {
        return;
    };
    // A jingle sounds over the quiet: it does not cut a track short, and
    // a track waits for its gap.
    if jingle.is_none()
        && let Some((playing, _)) = state.playing.take()
        && let Ok(mut entity) = commands.get_entity(playing)
    {
        entity.despawn();
    }
    info!("island_audio: music {} started", tune.stem());
    let entity = commands
        .spawn((
            Name::new("island music"),
            AudioPlayer::new(track),
            PlaybackSettings::DESPAWN.with_volume(Volume::Linear(levels.music_gain() * gain)),
        ))
        .id();
    if jingle.is_none() {
        state.tracks = state.tracks.wrapping_add(1);
        state.playing = Some((entity, gain));
        // The next track after this one and a long gap: a track is at most
        // two and a half minutes.
        state.next_at = now + 150.0 + mix::music_gap(state.tracks);
    }
}

/// The interface's sounds the UI asks for.
fn ui_sounds(
    mut commands: Commands,
    mut asked: MessageReader<PlayUiSound>,
    levels: Res<AudioLevels>,
    sounds: Option<Res<Sounds>>,
) {
    let Some(sounds) = sounds else {
        asked.clear();
        return;
    };
    let gain = levels.ui_gain();
    let mut played = BTreeSet::new();
    for PlayUiSound(sound) in asked.read() {
        // One of each a frame: a burst of the same request is one sound.
        if gain <= 0.0 || !played.insert(*sound) {
            continue;
        }
        if let Some((file, own)) = sounds.ui.get(sound) {
            info!("island_audio: ui {sound:?}");
            commands.spawn((
                Name::new("ui sound"),
                AudioPlayer::new(file.clone()),
                PlaybackSettings::DESPAWN.with_volume(Volume::Linear(gain * own)),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ui_widgets::Activate;

    #[test]
    fn one_emitter_keeps_its_cadence_and_a_busy_village_its_ceiling() {
        let mut limiter = ShotLimiter::default();
        let mut played = Vec::new();
        for frame in 0..600 {
            let now = frame as f32 / 60.0;
            if limiter.try_emit(Shot::Chop, 7, now, 5, 1.0) {
                played.push(now);
            }
        }
        assert!((7..=10).contains(&played.len()), "{played:?}");
        let mut limiter = ShotLimiter::default();
        let mut starts = Vec::new();
        for frame in 0..600 {
            let now = frame as f32 / 60.0;
            for emitter in 0..40 {
                if limiter.try_emit(Shot::Hammer, emitter, now, 5, 1.0) {
                    starts.push(now);
                }
            }
        }
        for (i, start) in starts.iter().enumerate() {
            let in_second = starts[i..]
                .iter()
                .take_while(|at| **at - *start < 1.0)
                .count();
            assert!(in_second <= 5, "burst at {start}");
        }
        // At ten times the speed, far fewer.
        let mut fast = ShotLimiter::default();
        let (per_second, wait) = mix::thinning(10);
        let thinned = (0..600)
            .flat_map(|frame| (0..40).map(move |emitter| (frame, emitter)))
            .filter(|(frame, emitter)| {
                fast.try_emit(
                    Shot::Hammer,
                    *emitter,
                    *frame as f32 / 60.0,
                    per_second,
                    wait,
                )
            })
            .count();
        assert!(thinned < starts.len() / 2, "{thinned} {}", starts.len());
        limiter.forget_idle(1000.0);
        assert!(limiter.next.is_empty());
    }

    #[test]
    fn the_older_settings_carry_into_the_levels_and_mute_silences() {
        let levels = AudioLevels::default();
        assert_eq!(levels.master, 0.8);
        assert_eq!(
            (levels.ambience, levels.effects, levels.music, levels.ui),
            (0.7, 0.7, 0.7, 0.7)
        );
        let carried = levels_from(IslandAudioSettings::default(), levels);
        assert_eq!(carried.master, 0.6);
        assert_eq!(carried.music, 0.7, "music keeps its own level");
        let muted = levels_from(IslandAudioSettings::muted(), levels);
        assert_eq!(
            (
                muted.ambience_gain(),
                muted.effects_gain(),
                muted.music_gain(),
                muted.ui_gain()
            ),
            (0.0, 0.0, 0.0, 0.0)
        );
        let loud = AudioLevels {
            master: 4.0,
            ..levels
        };
        assert_eq!(loud.effects_gain(), 1.0);
    }

    #[test]
    fn the_coast_is_found_by_a_coarse_ring() {
        // Water for x >= 20.
        let distance = mix_coast(Vec2::new(0.0, 0.0), |x, _| x >= 20);
        assert!((19.0..=24.0).contains(&distance), "{distance}");
        assert_eq!(mix_coast(Vec2::ZERO, |_, _| false), 200.0);
        assert_eq!(mix_coast(Vec2::ZERO, |_, _| true), 0.0);
    }

    /// A host without the audio plugin (headless, CI) gets no sound
    /// entities, and the UI can still send its sounds.
    #[test]
    fn a_headless_host_spawns_no_audio_and_takes_ui_requests() {
        let mut headless = App::new();
        headless.add_plugins(MinimalPlugins);
        plugin(&mut headless);
        headless.update();
        headless
            .world_mut()
            .write_message(PlayUiSound(UiSound::Confirm));
        let button = headless.world_mut().spawn_empty().id();
        headless.world_mut().trigger(Activate { entity: button });
        headless.update();
        let world = headless.world_mut();
        assert_eq!(world.query::<&AudioPlayer>().iter(world).count(), 0);
        assert_eq!(world.query::<&SpatialListener>().iter(world).count(), 0);
        assert!(world.contains_resource::<AudioLevels>());
        assert!(!world.contains_resource::<Sounds>());
    }
}
