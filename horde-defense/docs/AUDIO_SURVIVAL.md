# Survival sound effects

Last Light ships 281 original WAVs: six synthesized ambience beds and, since the wave-6 exporter (A6-05, 13 November 2026), 275 survival cue files in 63 banks under `assets/audio/survival_*.wav`. No music is shipped. Files are mono 44.1 kHz PCM16 WAV. No recordings, external samples, model-generated audio or additional runtime libraries are used. Original audio files and their attribution remain intact (`assets/CREDITS.md`).

The audio is presentation. Weapon selection and resolved simulation events choose cues; variants and jitter come from presentation counters and fixed seeds. Nothing here reads the core RNG, decides a hit or pays a reward.

## Loudness gate

A 60 s mix at default sliders reads **-23 ±2 LUFS integrated** with **true peak ≤ 0.95 FS**. This is the user's decision (U3, 2026-11-05) and replaces the brief's -16 to -14 LUFS. It is measured on the offline render of the shipped library under the runtime policy, metered with the BS.1770-4 meter in `horde_audio::meter` (4x oversampled true peak):

- `reference_mixed_60s_meets_loudness_gate`: 8 s Camp with two Confirm cues, 44 s of the seed-1 level-0 battle, 8 s Results with the victory stinger, on the stage-1 dry bed and on the rain bed.
- `dense_battle_60s_true_peak_at_most_0_95`: the maximum-progression battle fixture.
- `max_sliders_60s_is_logged`: every slider at 1.0, logged only (ignored).

Measured at A6-17 (2026-11-14) at the default sliders:

| Scene | Integrated (LUFS) | Short-term max | Momentary max | Sample peak (FS) | True peak (FS) |
|---|---|---|---|---|---|
| `reference_mixed_60s`, stage-1 | -24.2 | -21.5 | -19.1 | 0.390 | 0.405 |
| `reference_mixed_60s`, rain | -22.9 | -20.5 | -19.6 | 0.380 | 0.399 |
| `dense_battle_60s` | -23.1 | -21.6 | -19.1 | 0.823 | 0.875 |
| `max_sliders_60s` (logged) | -20.3 | -17.6 | -15.2 | 0.609 | 0.632 |

No runtime master limiter is used: the measured true peak stays under 0.95 FS (ruling R3). Before the production pass the same reference mix read -47.0 (stage-1) and -42.4 (rain) LUFS; the before and after tables are in the production-pass plan, section 9.3. Only the stage-1 dry and rain beds are gated; the wind, cold and urban beds are not.

## Library

| Banks | Files | What they are |
|---|---|---|
| 23 v1 families | 103 | The original cue families (table below), 17 with five variants and 6 with three |
| 5 gun-handling stages | 19 | Dry trigger and shell insert (five variants each); magazine out, magazine in and bolt (three each) |
| 24 enemy voices | 120 | Windup, contact and death for eight voices (shambler, runner, brute, armored, demolisher, swarm, spitter, exploder), five variants each |
| 9 boss banks | 27 | Arrival, phase and death for the Breacher, Caller and Siege Brute, three variants each |
| 2 interface families | 6 | Loot reveal and coin batch, three variants each |

A 24th v1 family, NpcShot, retired on 2026-11-06 (W1-58, user decision 2026-11-04): NPC defenders sound like their weapon class. Its three files are archived in `archive/audio/`, which is never packaged.

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

Some cues reuse another bank's files and differ from it only in their mix row and voicing: Kill, enemy death and boss death fall back to Impact; Victory plays the Wave bank and Defeat the Structure bank; Confirm, boss exposure and ability play the Ready bank (boss exposure 2 dB up and lower in pitch); Block plays the Armor bank 3 dB down. Every catalogue archetype names its enemy voice in `voicing::ENEMY_VOICES`, with an optional pitch and gain offset; an archetype without a row falls back to its legacy kind's voice. Elites and the Capstone boss play their bank lower (x0.94) and 1 dB louder. A loot cue's tone follows the item's quality tier (`voicing::quality_tone`); widget hover makes no sound.

A gun plays its staged reload (`gun.rs`): magazine out, in and the finishing Reload, or one shell insert per shell (at most six) for the shotgun, and the marksman's bolt 300 ms after a shot. An empty trigger with auto-reload off plays the dry trigger at most once per 350 ms; a held trigger clicks again every 400 ms.

## Buses and sliders

| Bus | Slider | Carries | Voices | Starts per frame |
|---|---|---|---|---|
| Master | Master | Scales every other bus | - | - |
| Effects | Combat effects | Every combat cue, warnings included | 8 | 3, plus the player-weapon lane |
| UI | Interface sounds | Confirm, loot reveal, coin batch | 3 | 2 |
| Music | Music & stingers | The victory, defeat and retreat stingers | 1 | 1 |
| Atmosphere | Atmosphere | The six ambience loops | 6 loops | - |

A voice plays at Master x its bus slider x the bus calibration x its row gain. The defaults are Master 0.8, Combat effects 0.8, Atmosphere 0.8, Music & stingers 0.7 and Interface sounds 0.7 (`mix::DEFAULT_*`). Settings version 2 moves an untouched wave-1 value (Master 0.6, Combat effects 0.7, Atmosphere 0.15) to its default once per field and keeps customised values. The "Mono audio" setting plays every voice centred.

Only Master and Combat effects can mute a danger cue: warnings ride the Effects bus, and neither Effects nor UI is ever ducked. Every warning has a visual twin (`mix::WARNING_TWINS`): an enemy windup has the windup ring and, when ranged, the aim line; a boss phase has the "PHASE n - NAME" label and ring.

The music director (`music.rs`) maps the screen and the battle's intensity to Silent, Menu, Camp, BattleLow, BattleHigh, Boss or Results. Every track slot is deferred (no music files, decision 9, user 2026-11-04; follow-up FU-AUD-MUSIC), so the Music bus plays only the stingers. A later track only fills a slot; the 2 s crossfade is implemented.

## Mix table

`horde_audio::mix::mix` holds every row; the runtime defines no gain of its own (`horde_game_audio_sources_define_no_gain_tables`). Priority 0 is the most important. The numbers at W7-03:

| Cues | Bus | Gain | Cooldown (ms) | Priority |
|---|---|---|---|---|
| Pistol, Rifle / Marksman / Shotgun | Effects | 0.9 / 0.9 / 1.0 | 80 / 120 / 120 | 9 |
| SMG / Machine gun | Effects | 0.75 / 0.9 | 55 | 9 |
| Turret shot | Effects | 0.65 | 150 | 11 |
| Impact / Armor / Block | Effects | 0.55 / 0.45 / 0.32 | 85 / 140 / 140 | 7 |
| Kill / Enemy death | Effects | 0.5 | 160 (one shared gate) | 6 |
| Damage / Melee / Structure / Trap | Effects | 0.75 / 0.65 / 0.75 / 0.75 | 240 / 180 / 550 / 220 | 3 / 4 / 2 / 8 |
| Explosion / Molotov / Fire | Effects | 1.0 / 0.65 / 0.3 | 180 / 180 / 850 | 2 / 2 / 13 |
| Reload / Throw / Ready | Effects | 0.55 / 0.55 / 0.45 | 180 / 180 / 400 | 5 / 4 / 6 |
| Magazine out, magazine in / Shell insert / Bolt / Dry trigger | Effects | 0.44 / 0.44 / 0.44 / 0.26 | 300 / 80 / 300 / 350 | 5 / 5 / 6 / 6 |
| Enemy windup / Enemy contact | Effects | 0.55 / 0.5 | 300 / 260 | 3 / 5 |
| Boss, Boss arrival / Boss phase / Boss death | Effects | 1.0 / 0.9 / 0.9 | 3000 / 1500 / 1500 | 0 / 1 / 2 |
| Boss exposed / Ability | Effects | 0.55 / 0.45 | 400 | 4 / 6 |
| Wave / Incoming | Effects | 0.65 / 0.3 | 1800 / 2400 | 1 / 12 |
| Loot drop | Effects | 0.5 | 400 | 6 |
| Victory, Defeat / Retreat | Music | 0.8 / 0.65 | 3000 / 1500 | 0 |
| Confirm / Loot reveal / Coin batch | UI | 0.45 / 0.5 / 0.4 | 250 / 400 / 600 | 3 / 4 / 5 |

Reduced effects plays every cue at 0.65 of its gain and makes every cooldown 3/2 as long. A cue's starts jitter in speed and gain within its group's bounds (guns 2% and 0.75 dB, enemy voices 4% and 1 dB, impacts 3% and 1 dB, boss 1%, interface 0.3 dB); jitter stays on in reduced effects because it adds no intensity.

## Selection, voices and pan

Each frame starts at most the bus's start budget, most important first; the root defender's own shot takes a reserved lane outside the budget. Some requests drop another of the same frame: a wave warning drops Incoming, a kill or enemy death drops its Impact, the defeat stinger drops the collapse and the victory stinger the wave call. Each bank rotates its variants round-robin from a hash of its trigger counter, so a variant never repeats twice in a row.

A bus holds its voice cap; a cue family holds one or two (`voices::Family::cap`: two each for guns, deaths, impacts, explosions, windups and contacts). A start into a full family steals that family's oldest voice; a start into a full bus steals the bus's least important voice, oldest first, and only gunfire, turret shots, impacts and the defender's own shot take an equally important one. A stolen voice fades out over 40 ms.

Positional cues (turret, impacts, armor, blasts, fire, structure, trap, incoming, kills, enemy voices, block and loot drops) pan along the lane from the defender at the left end of the field: `p = clamp((x - 0.5) / (width - 0.5), 0, 1)`. The law is peak-safe: the right channel stays at 1.0 and the left falls to `1 - D x p`, with `D` 0.30 (0.15 in reduced effects, 0 with Mono audio). At the far end that is 3.1 dB between the channels and 1.4 dB lost in a mono fold-down, so pan never raises a peak.

## Ducking and density

Stingers pull the Atmosphere bus 6 dB down while they sound; a boss arrival 6 dB for 1.2 s; a boss phase and a wave call 4 dB; an explosion, the loot reveal and the coin batch 3 dB. Ducks add up in dB to a -9 dB floor and release over 400-800 ms. Music tracks would duck 9 dB under a stinger or boss arrival; the stingers themselves are never ducked.

Up to four effect voices sound at their own level. Beyond that each compensated voice plays at `sqrt(4 / N)`, so the sum of N voices stops growing (10 ms attack, 250 ms release), and the ambience makes room: nothing up to two effect voices, then linearly down to -4 dB at six or more. The ambience loops play at 0.55 in the menu, 0.75 in camp, 1.0 in battle and 0.35 while paused, and crossfade between the six scene, biome and weather layers.

## Mastering and headroom

Each raw synthesis peaks at no more than 0.18 full scale. The exporter masters every clip offline (`synth/master.rs`): to -21 LUFS momentary-max under a 0.80 true-peak ceiling, with a lookahead limiter that may take at most 5 dB. Sixteen v1 families are short clicks and cracks that cannot reach -21 LUFS inside that bound; each is mastered to the loudest half-LU step its variants reach (-21.5 to -25.5 LUFS), recorded per file as `target_mmax_lufs`.

The runtime mix is calibrated against these mastered levels by two knobs (A6-17): `MIX_CALIBRATION_DB` -2.0 dB on the voice buses (Effects, UI and Music), the most the dense battle's true peak allows with margin, and `AMBIENCE_CALIBRATION_DB` +11.2 dB on the Atmosphere bus. The level-0 battle is one pistol and no music track ships, so the bed carries the integrated loudness of the reference mix. One voice at the maximum sliders stays under 0.75 FS and the loops under 0.57.

## Crate layout

The Bevy-free crate `crates/horde_audio` holds every audio decision: the cue list (`cue.rs`), banks and file paths (`bank.rs`), the mix table (`mix.rs`), the event-to-cue mapping (`events.rs`), voicing (`voicing.rs`), gun handling (`gun.rs`), loot and coins (`loot.rs`), selection and variety (`select.rs`), the voice pool (`voices.rs`), pan (`pan.rs`), ducking and density (`duck.rs`), the music director (`music.rs`), the offline renderer (`render.rs`), the meter (`meter.rs`), the review cue log (`cue_log.rs`) and the synthesizer and exporter (`synth/`, `export.rs`). `crates/horde_game/src/audio.rs` and its adapters only execute what the crate decides.

## Reproduce and verify

From the Horde Defense workspace:

```powershell
cargo test --locked -p horde_audio                                     # synthesis, mix, voices, pan, gate
cargo run --locked -p horde_audio --example export_audio              # check, writes nothing
cargo run --locked -p horde_audio --example export_audio -- --write   # regenerate
cargo test --locked -p horde_audio --test offline_mix -- --include-ignored --nocapture   # print the gate numbers
```

The exporter checks by default: it renders the library in memory, compares it with the shipped files and `assets/audio/SURVIVAL_PROVENANCE.json` (schema 2: seed, SHA-256, frames, peak, true peak, momentary-max loudness, mastering target and limiter gain reduction per file), prints one line per difference and exits 1 on any. `--write` regenerates the 275 files and the manifest, and writes an audition reel to `target/audio-audition/`, which is never committed. Run `--write` after changing a generator source (`generator_sources` in the manifest), then the ignored `asset_audit::regenerate_credits_index`. `export_check_is_idempotent` holds the shipped files to the export, and the v1 manifest is kept verbatim as `archive/audio/SURVIVAL_PROVENANCE.v1.json`. The six beds belong to the Python generators (`scripts/generate_audio.py` and `scripts/generate_atmosphere_audio.py`, `--check` by default) and `assets/audio/PROVENANCE.json`.

A review session started with `LASTLIGHT_REVIEW_CUE_LOG=<path>` writes every started and faded voice as JSON lines. `cargo run --locked -p horde_audio --example replay_cue_log -- <cues.jsonl> [--wav <out.wav>]` renders that log with the shipped clips and prints its loudness beside the gate, so a real session can be held against `reference_mixed_60s`.

The v1 audition reel (pre-production-pass), `docs/audio/survival-audition.wav` with its cue timeline `survival-audition.csv`, stays in the source tree only (ruling S19). It predates the mastered library and still plays the retired NpcShot family at 4.81 s, so it is a historical comparison, not the current sound; the playtest package leaves `docs/audio/` out. For the current library, run the exporter with `--write` and listen to `target/audio-audition/`.

## Listening pass: pending

These checks verify synthesis, scheduling and loudness properties. A human listening pass on a real device is **pending**; no listening, device-output, spatial or balance result is inferred from compilation, waveform inspection or the meter. The protocol and the fields to record are in `docs/PLAYTEST_GUIDE.md`, "Listening pass". One known risk for it: the bed reads about 4 LU over the level-0 gunfire in integrated terms. If it sounds too forward, the remedies that keep the gate are a music track or more voice gain with the onset pile-up removed, not a quieter bed alone.
