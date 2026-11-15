//! `MusicPlugin`: the Bevy side of the music director.
//!
//! The frontend publishes [`MusicTarget`] (the screen and the shown battle);
//! `horde_audio::music::MusicDirector` turns it into a music state, and this
//! plugin plays the track file of that state on the Music bus, crossfading on
//! a state change. Every track slot is deferred (no music files ship), so no
//! track plays; the Music bus carries the attempt stingers, which `SfxPlugin`
//! plays from existing banks at the "Music & stingers" level.
//!
//! Every number (thresholds, holds, the crossfade, the paused gain, the bus
//! gain, the duck) comes from `horde_audio`. Music is presentation: nothing
//! here reads the core RNG or changes game state, and it has no visual
//! effect to reduce.
use crate::settings::Settings;
use bevy::prelude::*;
use horde_audio::{
    duck::MAX_STEP_SECONDS,
    mix::Bus,
    music::{MusicDirector, MusicInput, MusicState, TRACKS, Track, TrackVoice},
};

/// What the director scores: published by the frontend every frame.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct MusicTarget(pub MusicInput);

/// The director and its track slots. The F3 panel reads [`Self::report`].
#[derive(Resource, Clone, Debug)]
pub struct MusicDirection {
    director: MusicDirector,
    /// `horde_audio::music::TRACKS`: every slot deferred.
    tracks: [(MusicState, Track); 7],
    /// Real time not yet handed to the director, in ms (below 1).
    carry_ms: f64,
}
impl Default for MusicDirection {
    fn default() -> Self {
        Self {
            director: MusicDirector::default(),
            tracks: TRACKS,
            carry_ms: 0.0,
        }
    }
}
impl MusicDirection {
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn state(&self) -> MusicState {
        self.director.state()
    }
    /// Advances the director by `dt_ms` of real time.
    pub fn advance(&mut self, input: &MusicInput, dt_ms: u32) -> MusicState {
        self.director.update(input, dt_ms)
    }
    /// The tracks to play now, before the bus gain and the duck.
    fn voices(&self) -> Vec<TrackVoice> {
        self.director.voices(&self.tracks)
    }
    /// The director's state for the F3 panel.
    #[cfg_attr(not(any(test, feature = "dev")), allow(dead_code))]
    pub fn report(&self) -> String {
        let state = self.director.state();
        let track = match self
            .tracks
            .iter()
            .find(|(slot, _)| *slot == state)
            .map(|(_, track)| *track)
        {
            Some(Track::File(path)) => format!("track {path}"),
            _ => "track deferred".to_owned(),
        };
        format!(
            "{}, intensity {:.2}, {track}",
            state.name(),
            self.director.intensity()
        )
    }
}

/// One looping track file.
#[derive(Component, Clone, Copy, Debug)]
pub struct MusicTrack {
    pub path: &'static str,
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct MusicUpdate;

pub struct MusicPlugin;
impl Plugin for MusicPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MusicTarget>()
            .init_resource::<MusicDirection>()
            .add_systems(Update, direct_music.in_set(MusicUpdate));
    }
}

#[allow(clippy::too_many_arguments)] // a Bevy system: each parameter is a world access
fn direct_music(
    mut commands: Commands,
    assets: Res<AssetServer>,
    target: Res<MusicTarget>,
    settings: Res<Settings>,
    ducking: Option<Res<crate::sfx::Ducking>>,
    time: Res<Time<Real>>,
    mut direction: ResMut<MusicDirection>,
    mut tracks: Query<(
        Entity,
        &MusicTrack,
        &mut PlaybackSettings,
        Option<&mut AudioSink>,
    )>,
) {
    // A stall does not skip a crossfade or a hold.
    let step = f64::from(time.delta_secs().clamp(0.0, MAX_STEP_SECONDS)) * 1000.0;
    let elapsed = direction.carry_ms + step;
    let whole = elapsed.floor();
    direction.carry_ms = elapsed - whole;
    direction.advance(&target.0, whole as u32);
    let voices = direction.voices();
    if voices.is_empty() && tracks.is_empty() {
        // Every slot deferred: nothing to play and nothing playing.
        return;
    }
    // The tracks follow "Music & stingers" and the Music duck (a stinger or
    // a boss arrival pulls them down; the stingers themselves never duck).
    let bus = settings.sliders().bus_gain(Bus::Music)
        * ducking.map_or(1.0, |ducking| ducking.gains.duck[Bus::Music as usize]);
    for (entity, track, mut playback, sink) in &mut tracks {
        let Some(voice) = voices.iter().find(|voice| voice.path == track.path) else {
            // Faded out, or its state left without a crossfade partner.
            commands.entity(entity).despawn();
            continue;
        };
        let volume = bevy::audio::Volume::Linear(voice.gain * bus);
        // The component covers a sink not created yet; the sink a playing loop.
        playback.volume = volume;
        if let Some(mut sink) = sink {
            sink.set_volume(volume);
        }
    }
    for voice in &voices {
        if tracks.iter().any(|(_, track, ..)| track.path == voice.path) {
            continue;
        }
        commands.spawn((
            MusicTrack { path: voice.path },
            AudioPlayer::new(assets.load::<AudioSource>(voice.path)),
            PlaybackSettings::LOOP.with_volume(bevy::audio::Volume::Linear(voice.gain * bus)),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use horde_audio::{
        mix::Sliders,
        music::{BattleInput, CROSSFADE_MS, HIGH_HOLD_MS, MusicScreen},
    };
    use std::time::Duration;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::asset::AssetPlugin::default()))
            .init_asset::<AudioSource>()
            .insert_resource(Settings::default())
            .add_plugins(MusicPlugin);
        app
    }
    /// One frame `ms` of real time long.
    fn frame(app: &mut App, ms: u64) {
        app.world_mut()
            .resource_mut::<Time<Real>>()
            .advance_by(Duration::from_millis(ms));
        app.world_mut().run_system_cached(direct_music).unwrap();
        app.world_mut().flush();
    }
    fn tracks(app: &mut App) -> Vec<(&'static str, f32)> {
        let mut query = app.world_mut().query::<(&MusicTrack, &PlaybackSettings)>();
        let mut found: Vec<(&'static str, f32)> = query
            .iter(app.world())
            .map(|(track, playback)| (track.path, playback.volume.to_linear()))
            .collect();
        found.sort_by(|a, b| a.0.cmp(b.0));
        found
    }
    fn fierce() -> MusicInput {
        MusicInput {
            screen: MusicScreen::Battle,
            battle: Some(BattleInput {
                enemies_alive: 40,
                nearest_x_frac: 0.0,
                hp_frac: 0.5,
                boss_alive: false,
                paused: false,
            }),
        }
    }

    /// The plugin directs on real time, and with every slot deferred it
    /// plays no track in any state.
    #[test]
    fn the_director_follows_the_target_and_plays_no_deferred_track() {
        let mut app = app();
        frame(&mut app, 16);
        assert_eq!(
            app.world().resource::<MusicDirection>().state(),
            MusicState::Menu
        );
        app.world_mut().resource_mut::<MusicTarget>().0 = fierce();
        frame(&mut app, 16);
        assert_eq!(
            app.world().resource::<MusicDirection>().state(),
            MusicState::BattleLow
        );
        // 2 s of held intensity in 100 ms frames: a stalled frame counts as
        // 250 ms at most, so one long frame does not skip the hold.
        frame(&mut app, 10_000);
        assert_eq!(
            app.world().resource::<MusicDirection>().state(),
            MusicState::BattleLow
        );
        for _ in 0..HIGH_HOLD_MS / 100 {
            frame(&mut app, 100);
        }
        assert_eq!(
            app.world().resource::<MusicDirection>().state(),
            MusicState::BattleHigh
        );
        assert!(tracks(&mut app).is_empty());
        assert_eq!(
            app.world().resource::<MusicDirection>().report(),
            "BattleHigh, intensity 0.90, track deferred"
        );
    }

    /// `Track::File` playback: a slot with a file loops it at the Music bus
    /// gain, crossfades on a state change and ends the old loop.
    #[test]
    fn a_track_file_loops_at_the_music_bus_gain_and_crossfades() {
        let mut app = app();
        {
            let mut direction = app.world_mut().resource_mut::<MusicDirection>();
            for (state, slot) in &mut direction.tracks {
                *slot = match state {
                    MusicState::Menu => Track::File("audio/test_menu.wav"),
                    MusicState::BattleLow => Track::File("audio/test_battle.wav"),
                    _ => *slot,
                };
            }
        }
        let settings = Settings {
            music: 0.5,
            ..Settings::default()
        };
        let bus = settings.sliders().bus_gain(Bus::Music);
        assert_eq!(
            bus,
            Sliders {
                music: 0.5,
                ..Sliders::DEFAULT
            }
            .bus_gain(Bus::Music)
        );
        app.insert_resource(settings);
        frame(&mut app, 0);
        assert_eq!(tracks(&mut app), vec![("audio/test_menu.wav", 0.0)]);
        for _ in 0..CROSSFADE_MS / 250 {
            frame(&mut app, 250);
        }
        let playing = tracks(&mut app);
        assert_eq!(playing.len(), 1);
        assert!((playing[0].1 - bus).abs() < 1e-6, "{playing:?}");
        {
            let mut query = app.world_mut().query::<&PlaybackSettings>();
            let playback = query.single(app.world()).unwrap();
            assert!(matches!(playback.mode, bevy::audio::PlaybackMode::Loop));
        }
        // Menu to the battle: both loops, summing to the bus gain.
        app.world_mut().resource_mut::<MusicTarget>().0 = MusicInput {
            battle: fierce().battle.map(|battle| BattleInput {
                enemies_alive: 0,
                ..battle
            }),
            ..fierce()
        };
        frame(&mut app, 250);
        frame(&mut app, 250);
        let playing = tracks(&mut app);
        assert_eq!(playing.len(), 2, "{playing:?}");
        assert!((playing[0].1 + playing[1].1 - bus).abs() < 1e-6);
        assert!(playing[0].1 > 0.0 && playing[1].1 > 0.0);
        for _ in 0..CROSSFADE_MS / 250 {
            frame(&mut app, 250);
        }
        let playing = tracks(&mut app);
        assert_eq!(playing.len(), 1, "{playing:?}");
        assert_eq!(playing[0].0, "audio/test_battle.wav");
        assert!((playing[0].1 - bus).abs() < 1e-6);
        // The slider reaches a playing loop.
        app.world_mut().resource_mut::<Settings>().music = 0.0;
        frame(&mut app, 16);
        assert_eq!(tracks(&mut app), vec![("audio/test_battle.wav", 0.0)]);
        // A deferred state fades the loop out and ends it.
        app.world_mut().resource_mut::<MusicTarget>().0 = MusicInput {
            screen: MusicScreen::Results,
            battle: None,
        };
        for _ in 0..=CROSSFADE_MS / 250 {
            frame(&mut app, 250);
        }
        assert!(tracks(&mut app).is_empty());
    }
}
