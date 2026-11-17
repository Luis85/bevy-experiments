# Atmosphere, sound and presentation review — 2 November 2026

This pass adds environmental audio and source feedback around the existing game. It does not change spawning, movement, collision geometry, NPC rules, rewards, progression or saved random state. Weather/actor pixels are owned by the separate animation pass; sound and visual weather share the same pure scene classification.

## Implemented audio policy

`atmosphere_audio.rs` contains `SceneAmbience`, `SceneAudioTarget` and `AtmosphereAudioPlugin`. The frontend publishes a target from the actual active or explicitly observed scene. It owns no audio entities. The plugin creates exactly six persistent loop channels once: the original quiet base, wind, urban detail, daytime nature, rain and cold air. Pending decoders use updated `PlaybackSettings`; already playing sources use `AudioSink` volume updates. Scene changes do not create another set of players.

Scene weights are finite, nonnegative and normalized to a sum no greater than one. They are multiplied by independent master/ambience values. Effects volume is not an environmental input. A 1.2-second convex smoothstep blends between targets without a gain overshoot. Menu, camp and pause contexts duck the bed. Explicit master or ambience mute is immediate, including pending sinks. Observation speed does not change ambience pitch, and reduced visual effects does not silently mute the audio bus.

Authored rain/drizzle/storm selects rain, cold regions or snow select cold air, and location tags such as depot, industrial yard or checkpoint add distant mechanical texture. Nature calls are excluded at night and in cold/arid settings. Explicit clear/calm tags beat precipitation fallback. The same physical scene has the same environment in personal and NPC mode; its combat events distinguish the active source.

## Original assets and reproducibility

The original nine audio files were not rewritten. Five new loops and four source cues are produced by `scripts/generate_atmosphere_audio.py` using Python's standard library, fixed seeds, cycle-complete oscillators and cyclic filtered noise. There are no recordings or third-party samples. These are fictional environmental/weapon textures, not researched national soundscapes or specific real weapon recordings.

| New clip | Duration | Signed 16-bit peak |
|---|---:|---:|
| Ambient wind | 16 s | 17,767 |
| Ambient urban | 20 s | 8,597 |
| Ambient nature | 24 s | 18,545 |
| Ambient rain | 18 s | 16,001 |
| Ambient cold | 22 s | 13,753 |
| NPC shot | 0.26 s | 16,848 |
| Turret shot | 0.20 s | 15,540 |
| Trap trigger | approximately 0.65 s | 18,768 |
| Armor impact | approximately 0.15 s | 7,642 |

All eighteen WAVs pass mono, 16-bit, 22,050 Hz uncompressed PCM checks, nonempty sample and below-clipping checks. The generator was run twice; all eighteen WAV hashes were identical between runs, and the original nine hashes stayed unchanged. `assets/audio/PROVENANCE.json` records exact frame counts, RMS levels, seeds, generators and SHA-256 values.

Archived on 6 November 2026: the eighteen WAVs above no longer all ship. W1-57 (production-pass decision 10) moved twelve of them unchanged to `archive/audio/`: the eight original effect clips (shot, reload, melee, damage, boss, incoming, ready, structure) and the four new source cues (NPC shot, turret shot, trap trigger, armor impact), see `archive/README.md` and `assets/CREDITS.md` ("Archived (not shipped)"). `assets/audio/` keeps the six beds this review describes: the original quiet base `ambience.wav` and the five new loops. `assets/audio/PROVENANCE.json` now records only those six; the twelve archived records, with the fields listed above, are in `archive/audio/PROVENANCE.json`. Combat effects now come from the survival library in [AUDIO_SURVIVAL.md](AUDIO_SURVIVAL.md). The table and hash results above record the eighteen files as they were measured.

New loop boundary changes are checked against the larger of 0.004 full scale and that clip's 99th-percentile ordinary adjacent-sample change. This checks that wrapping does not introduce a special impulse above the existing noise texture; it does not pretend a rain waveform must end at zero. The five loops pass. This is a waveform check, not evidence that a listener cannot notice repetition.

## Read-only flow findings sent for runner integration

| Priority | Finding in the prior source | Integration requirement |
|---|---|---|
| P1 | Boss warnings/reinforcement announcements shared ordinary arrival's lowest audio priority, so a simultaneous shot could suppress the warning. | Warning priority must be independent of the reused clip path and exceed routine fire. |
| P2 | Armor absorption had a visible event but no mapped sound. | Route the authoritative Armor event to its distinct short cue; do not imply health damage. |
| P2 | Already playing effects retained their spawn-time volume when master/effects changed. | Update pending playback and active effect sinks so muting cannot leave a tail or delayed loading burst. |
| P2 | An explicitly watched parallel battle still filtered all ordinary fire out of the camp audio path. | Audition the watched battle, while leaving other workers critical-alert-only. |
| P2 | A single global cue choice can replace a gunshot with its armor impact, or permanently hide a turret whose one-second shots align with the NPC. | Use bounded cue families/voices and authoritative source IDs, rather than an unlimited mix or a second combat resolver. |

The suspected observation-speed leak into personal mode was checked and was **not present**: deployment/retry reset speed, and the control publisher explicitly forces personal speed to one. This review does not manufacture a new issue from that path.

## Verification boundary

Six pure unit cases cover authored-layer selection, clear-night behavior, identical physical environment across control modes, bounded crossfade gain, immediate mute, display-rate-independent transition duration and context ducking. The source was formatted and the Python generator parsed successfully. Native integration, combined Cargo results and the final runner mappings are recorded after the parent integration gate; these added tests are not represented here as already executed before that gate.

No speaker/headphone listening session is claimed from PCM inspection. Human tests should assess source recognition, masking, loops, pause behavior and the default mix at normal listening levels. The playtest guide provides that sequence. Audio does not replace the persistent visual warnings and logs, and paused ambient playback does not advance authoritative game time.

Parent integration passed the final 185-test workspace gate and strict Clippy. The runner now prioritizes warnings, distinguishes trap/NPC/turret/armor cues, bounds effects to 24 voices and updates pending/live effect gains. A fresh independent review found and confirmed the trap-source correction. Native active/paused renderer runs completed with the atmosphere plugin; [VALIDATION.md](VALIDATION.md) records the measured frame intervals. This establishes integration and asset loading, not speaker/headphone mix quality.
