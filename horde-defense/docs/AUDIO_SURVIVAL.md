# Survival sound effects

The new `assets/audio/survival_*_[0–2].wav` library contains 23 shipped original effects with three repeatable variations each (69 files). A 24th family, NpcShot, retired on 2026-11-06 (W1-58, user decision 2026-11-04; NPC defenders now sound like their weapon class); its three files are archived in `archive/audio/`. Files are mono 44.1 kHz PCM16 WAV. No recordings, external samples or additional runtime libraries are required. Original audio files and their attribution remain intact. `assets/audio/SURVIVAL_PROVENANCE.json` records every new file's SHA-256, duration, peak and RMS level.

Listen to [the v1 audition reel (pre-production-pass)](audio/survival-audition.wav). Its [CSV timeline](audio/survival-audition.csv) identifies each cue; the reel plays variant zero with a third-second gap. It is not the current library: the committed reel and CSV were exported before NpcShot retired, so they still play that archived family at 4.81 s. A re-export iterates only the shipped families and drops it. Production-pass ruling S19 keeps this v1 reel in `docs/audio/` and, from wave 6, sends new reels only to the uncommitted `target/audio-audition/`. The reel is an authoring comparison at the library's recorded level, before the in-game master, effects and cue gains.

| Sound family | Design |
| --- | --- |
| Pistol | Short pressure body and dry high-frequency crack |
| SMG | Shorter, brighter report that stays readable at rapid cadence |
| Shotgun | Broad low-frequency body and longer diffuse decay |
| Rifle | Hard narrow transient with a medium outdoor reflection |
| Marksman | Heavier pressure body and longest outdoor decay |
| Machine gun | Low mechanical body and rough midrange |
| Turret | Report with a mechanical action click |
| Impact / armor | Dull physical strike / short irregular metallic resonance |
| Reload / ready | Magazine, sliding mechanism and latch transients; no musical chime |
| Throw / grenade | Small equipment click and cloth/air motion / pressure, debris and reflected noise |
| Molotov / fire | Irregular glass fragments and liquid impact / restrained noisy crackle |
| Retreat / wave / incoming | Short radio squelch and relay clicks |
| Boss / structure | Low industrial warning / layered physical collapse |

Four presentation cues reuse a synthesized family's files instead of adding new ones. They differ from their bank only in mixer priority, gain and repeat gate, and the exporter never synthesizes them:

| Cue | Bank | Trigger | Mix |
| --- | --- | --- | --- |
| Kill | Impact | An enemy dies (`SimEvent::Kill`) | Gain 0.5, below a hit; gated to one per 160 ms; replaces that update's impact |
| Victory | Wave | `AttemptFinished` with a victory | Top priority, gain 0.8; replaces that update's wave call |
| Defeat | Structure | `AttemptFinished` with a defeat | Top priority, gain 0.8; replaces that update's collapse |
| Confirm | Ready | A successful skill-node, item, training, recruit or equip command | Gain 0.45; gated to one per 250 ms |

A withdrawal plays the existing Retreat cue once, from its `AttemptFinished` event. A `Withdraw` command drains that event immediately because it pauses the game.

The Bevy-free crate `crates/horde_audio` defines the cue list (`cue.rs`), the banks and file paths (`bank.rs`) and the offline synthesizer (`synth/legacy.rs`). The mix table (`mix.rs`), the event-to-cue mapping (`events.rs`), cue selection and the repeat gate (`select.rs`) and the offline mix renderer (`render.rs`) live there too. `crates/horde_game/src/audio.rs` keeps the cue message. Neither contains simulation calls. Weapon selection and resolved events choose cues; procedural variations use presentation counters and their own fixed seeds. They never consume combat randomness, determine hits or pay rewards.

The mixer caps effects at six concurrent voices and selects at most two new cues per update. Real-time per-cue gates suppress repeated damage/arrival/fire feedback when many simulation ticks advance together. Important warnings retain their existing visual equivalents. Pending and playing voices follow master/effects changes, including immediate mute; reduced effects lowers individual gains and increases minimum repeat intervals. The existing separate ambience bus continues to crossfade the six scene/biome/weather layers, including quieter menu and paused contexts.

Each waveform peaks at no more than 0.18 full scale. At the maximum effects bus gain of 0.7, six perfectly aligned effects contribute no more than 0.756 full scale before the quieter ambience bed. Normal per-cue gains reduce this further. This is conservative digital headroom, not a loudness standard or a guarantee about a listener's hardware gain.

## Reproduce and verify

From the Horde Defense workspace, run the synthesis tests through Cargo. They live in `horde_audio`, which never compiles Bevy. `raw_synthesis_matches_v1_library` holds the synthesis to the recorded sha256 of every v1 file.

```powershell
cargo test --locked -p horde_audio
$env:LASTLIGHT_AUDIO_EXPORT = (Get-Location).Path
cargo test --locked -p horde_audio --test legacy_library export_survival_audio_assets_and_audition -- --ignored --exact
```

The export explicitly regenerates the 69 shipped files and audition reel (the retired NpcShot family stays archived in `archive/audio/`); ordinary tests never write those artifacts. Regenerate the provenance hashes when changing synthesis. The normal workspace tests include the `horde_audio` tests and the gain and gate tests in the game's `audio::tests`; the export test remains ignored unless explicitly invoked.

Tests check deterministic variants, distinct weapon decay/energy profiles, finite samples, DC offset, zero-valued endpoints, useful RMS levels, effect headroom and repeat gating. Independent WAV inspection confirmed all 69 shipped encoded files have the expected format and endpoints; their peak is at most 0.179998 and RMS spans 0.008312–0.057572 before mixer gains.

These checks verify synthesis and scheduling properties. A human listening pass, device output, spatial perception and balance against real gameplay remain separate validation. No listening or device-output result is inferred from compilation or waveform inspection.
