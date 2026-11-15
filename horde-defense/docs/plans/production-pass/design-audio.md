# Audio production pass: synthesized spec (goal 8, decisions 9, 10 and 17)

All paths are relative to `horde-defense/`.

**Base and inputs.** All three judges picked design B (scores 8 / 8 / 7.6, against A's 5.5 / 6.5 / 6.2). B is the base. Per-decision majority preferences and grafts are applied below.

**User-binding inputs:**
- -23 +/-2 LUFS integrated with true peak <= 0.95 FS, from a 60 s mix at default sliders. This replaces the brief's "-16 to -14".
- The exporter may make new gun, enemy and UI audio, but no music files and no ImageGen sheets.
- MusicPlugin is a director. Every track slot is Deferred, and the slider reads "Music & stingers".
- A limiter is allowed only if the measured TP > 0.95.

## 0. Verified facts this spec relies on

- **Kill fires after the enemy is removed.** `reap` calls `a.enemies.remove(n)` at `crates/horde_core/src/sim/combat.rs:322`, pushes a `CorpseRecord {id, kind, boss_kind, at, visual, tick}` at :326-333, and only then emits Kill at :357-367. So no core-side lookup can find a Kill's archetype. Design A's EnemyRef-in-`typed()` is rejected on this ground.
- **Archetype id location.** It lives at `Enemy.resolved: Option<ResolvedEnemy>` with `ResolvedEnemy.archetype_id: String` (`sim.rs:94-128`, `enemy_catalog.rs:49-61`). CorpseRecord has no archetype. Catalog ids today: shambler, runner, brute, armored, demolisher, breacher-pack, caller-pack, siege-pack, skitter, plated-runner, hungry-shambler (11). `EnemyCatalog::archetype_ids()` is at `enemy_catalog.rs:176`.
- **BossKind** is `{Breacher, Caller, SiegeBrute}` (`content.rs:46-50`). Decision 12 adds no new BossKinds, so the capstone is NOT a bank key.
- **Workspace.** `Cargo.toml` members are `[crates/horde_core, crates/horde_game]`, with `unsafe_code = "forbid"`. Adding `crates/horde_audio` stays under horde-defense/.
- **Fixture path.** The dense fixture's public path is `horde_core::sim::maximum_progression_render_fixture` (`sim.rs:18`, defined at `sim/fixtures.rs:77`). The `fixtures` module is private.
- **Settings.** The container is `#[serde(default)]` (`settings.rs:42-43`). `Settings::load` falls back to the default on parse failure (:79-84). The old defaults are master 0.6, effects 0.7, ambience 0.15 (:71-73), and `bindings: [String; 8]` is at :57/:74.
- **Settings naming.** The ux-flow audit names `Settings.ui_volume`; no audit names the music field. Chosen: `ui_volume` and `music_volume`, a consistent suffix. Judge 3's claim that ux uses `Settings.music` is not borne out by `wave0/ux-flow.json`.
- **SimEvent** has 25 variants: Shot, Hit, WeakPoint, Armor, Miss, Defended, Damage, Melee, Kill, Reload, Spawn, Boss, StructureLost, Ability, Contact, Windup, BossWarning, BossExposure, Reinforcements, Throw, Explosion, Burn, CooldownReady, RuleResolved, AttemptFinished. SimEvent is not serialized. GAMEPLAY_TRACE hashes `Attempt.events` (`tests/determinism.rs:82-99`), and `emitted` is `serde(skip)` (`sim.rs:424`).
- **Cue enum.** 28 cues with a manual `CUE_COUNT = 28` (`audio.rs:8-43`). Gain and cooldown tables are at `audio.rs:86-124`, the variant path `% 3` at :126-128, PEAK 0.18 at :160, the seed at :268, and the effects-only headroom assert at :477.
- **sfx.rs.** The hidden x0.7 is at sfx.rs:76-85. Selection allows 2 starts per frame (:86-136), there is one global rotation (:150-151, :192), and stealing despawns instantly (:160-181).
- **runner.** `run_tick` (`runner/runtime.rs:217-241`) calls `game.tick(input)` and then `drain_sounds` → `tick_sounds`. `event_sound` is at :358-425; Npc→NpcShot at :390-392; BossExposure maps to Ready in both states at :376; the `_ => None` wildcard is at :420. Expedition audibility is at :430-473.
- **Rare-loot cue.** It is fired from the widget draw (`ui/loot.rs:150` → `ui/context.rs:255`, Ready alias) and from `ui/operations.rs:303`. Core `attempt.loot_events` (`quality.rs:194-203`, capped at MAX_LOOT_EVENTS) carries `at`, `quality`, `item_id` and `tick`.
- **Measured** (read-only Python, design-A scratchpad `...\scratchpad\audio_design\`):
  - shipped survival files read -26.0 to -38.3 LUFS momentary-max at 0.18 FS peak (pistol_0 -35.9), a peak-to-loudness ratio of 11-23 dB;
  - today's defaults model at about -42 LUFS integrated in a scripted 60 s battle;
  - the recipe in section 3 models -23.2 LUFS with TP 0.57 at the new defaults, and TP 0.89 at max sliders;
  - the Python loops re-render bit-exact (9/9).

## 1. Architecture (B base)

### New workspace crate `crates/horde_audio`

- Dependencies: `horde_core`, `serde`, `serde_json`. All are already in the lock graph. No Bevy, no new external crates, `unsafe_code` forbidden.
- It owns every decision and every table. `cargo test -p horde_audio` never compiles Bevy.

| Module | Contents |
|---|---|
| `sha256.rs` | Std-only FIPS 180-4, public. Shared with the vfx goal-7 art-provenance test. |
| `wav.rs` | PCM16 read and write. |
| `provenance.rs` | Schema-2 parse and merge-write. |
| `cue.rs` | `Cue`, `Cue::ALL`, `CUE_COUNT = Cue::ALL.len()`. |
| `bank.rs` | `BankId`, file paths, `ENEMY_VOICES`, `BOSS_VOICES`, `AMBIENCE_FILES`. |
| `mix.rs` | Every gain, cooldown, priority, bus, cap, duck, trim, reduced column, `QUALITY_TONE`, `WARNING_TWINS`, `CONTEXT_GAIN` (moved from `atmosphere_audio.rs:55-62`), `MIX_CALIBRATION_DB`, `AMBIENCE_CALIBRATION_DB`, and the default slider constants. |
| `select.rs` | `CueGate`, `RoundRobin`, `splitmix64`, jitter. |
| `mixer.rs` | `Mixer` state machine. Takes `SoundRequest`s, returns `FrameActions`. |
| `dsp.rs` | Pan law, one-pole low-pass, gain ramp. Shared verbatim by runtime and renderer. |
| `events.rs` | `event_requests`, `EventContext`, `EnemyVoiceIndex`, `GunTracker`, `LootTracker`. |
| `music.rs` | `MusicDirector`, `MusicState`, `TRACKS`. |
| `meter.rs` | BS.1770-4 meter. |
| `render.rs` | Offline renderer. |
| `synth/{legacy,gun,enemy,boss,ui,master}.rs` | Synthesis and mastering. |
| `examples/export_audio.rs` | The exporter. |
| `examples/replay_cue_log.rs` | Optional (step 6.18). |

### horde_game keeps only the Bevy adapter

- `audio.rs`: the `CueRequest` Message, `AudioBanks` preload of every variant `Handle<AudioSource>`, and the `PannedClip` Decodable.
- `sfx.rs`: executes `FrameActions`.
- `atmosphere_audio.rs`: loops. It multiplies in the duck gain.
- New `music.rs`: `MusicPlugin` and the `MusicTarget` resource, published next to `ambience_target` (`ui/context.rs:121-166`).

**Fallback.** If perf-architecture or packaging rejects the crate, the same modules live under `crates/horde_game/src/audio/` as `cfg(test)`. The exporter then runs via the existing cargo-test route (`LASTLIGHT_AUDIO_EXPORT=<root> cargo test --locked -p horde-defense -- --ignored --exact audio::authoring::export::tests::export_audio_library`). Module names and tests are unchanged.

### Types

```rust
// horde_audio::cue  (append-only; existing discriminants 0..=27 unchanged)
pub enum Cue { Pistol, Smg, Shotgun, Rifle, Marksman, MachineGun, NpcShot /*retired*/, TurretShot, Impact, Armor,
  Reload, Throw, Explosion, Molotov, Fire, Retreat, Boss, Wave, Damage, Melee, Ready, Structure, Trap, Incoming,
  Kill, Victory, Defeat, Confirm,
  // appended in wave 6:
  DryFire, ReloadOut, ReloadIn, ShellInsert, BoltCycle, EnemyWindup, EnemyContact, EnemyDeath,
  BossArrival, BossPhase, BossDeath, BossExposed, Block, Ability, LootDrop, LootReveal, CoinBatch }
pub enum Bus { Master, Effects, Ui, Music, Atmosphere }
pub enum BankId { Legacy(Cue), Gun(GunStage), Enemy(EnemyVoice, EnemyStage), Boss(BossKind, BossStage), Ui(UiFamily) }
pub enum EnemyVoice { Shambler, Runner, Brute, Armored, Demolisher, Swarm, Spitter, Exploder }
pub enum EnemyStage { Windup, Contact, Death }   pub enum BossStage { Arrival, Phase, Death }
pub enum GunStage { DryFire, ReloadOut, ReloadIn, ShellInsert, BoltCycle }   pub enum UiFamily { LootReveal, CoinBatch }
pub struct CueMix { bus: Bus, gain_db: f32, cooldown_ms: u32, priority: u8, gate: GateGroup, max_concurrent: u8,
  positional: bool, reduced_gain_db: f32 /*default -3.7*/, reduced_cooldown_x: f32 /*default 1.5*/, retired: bool,
  warning: Option<VisualTwin>, jitter: JitterBounds }
pub const fn mix(cue: Cue) -> CueMix   // exhaustive match, no wildcard
pub struct SoundRequest { cue: Cue, bank: BankId, pan: f32 /*0..=1, 0 = centre*/, speed: f32, gain_db: f32,
  lowpass_hz: Option<f32>, delay_ms: u16, source: VoiceSource /*RootDefender|Field|Observed|Ui|Music*/ }
impl PartialEq<Cue> for SoundRequest   // existing `assert_eq!(cues, vec![SoundCue::X])` tests compile unchanged
pub struct FrameActions { starts: Vec<VoiceStart>, fades: Vec<(VoiceId, f32 /*ms*/)>, bus_gains: [f32; 5], duck: [f32; 5] }
```

- `CueRequest` (the horde_game Message, `runner/plugin.rs:29`) wraps `SoundRequest`.
- About 15 `.contains(&Cue)` test sites change mechanically to `.iter().any(|r| r.cue == X)`.
- `push_cue` keeps per-batch dedup by `(cue, bank)`. For warnings, the request nearest the defender wins.

## 2. Banks and files

**Legacy families (24 today).**
- Cues with cooldown <= 250 ms (directly or via an alias) go to **5 variants**: Pistol, Smg, Shotgun, Rifle, Marksman, MachineGun, TurretShot, Impact, Armor, Reload, Throw, Explosion, Molotov, Damage, Melee, Ready (Confirm alias at 250 ms), and Trap (17 x 5 = 85).
- Fire, Retreat, Boss, Wave, Structure and Incoming stay at 3 (18).
- **NpcShot is retired.** `CueMix.retired = true`, the enum slot is kept, and `survival_npc_shot_{0..2}.wav` move to `archive/audio/retired/` with their provenance. This is recorded separately from decision 10's 12 files.
- Seed: `0x85eb_ca6b * (1 + bank_ordinal*17 + v)`, with `v < 17` asserted. The `% 3` goes away.
- Shift table `[0.96, 1.00, 1.04, 0.92, 1.08]`. Variants 0-2 are bit-identical before mastering.

**New Rust-synthesized families.** All are non-melodic: knocks, noise bands, inharmonic clinks. File names use the `survival_` prefix, which marks exporter ownership.

| Family | Variants | Files |
|---|---|---|
| `survival_gun_dryfire_<v>` | 5 | 5 |
| `survival_gun_reload_out_<v>` | 3 | 3 |
| `survival_gun_reload_in_<v>` | 3 | 3 |
| `survival_gun_shell_insert_<v>` (shotgun) | 5 | 5 |
| `survival_gun_bolt_cycle_<v>` (marksman) | 3 | 3 |
| `survival_enemy_<voice>_<stage>_<v>`, 8 voices x 3 stages | 5 | 120 |
| `survival_boss_<kind>_<stage>_<v>`, 3 BossKind x 3 stages | 3 | 27 |
| `survival_ui_loot_reveal_<v>` | 3 | 3 |
| `survival_ui_coin_batch_<v>` | 3 | 3 |

- That is 172 new files, plus 103 re-mastered legacy files, for 275 survival WAVs. With the 6 Python loops, 281 WAVs ship (about 17-19 MB, up from 9.47 MB). The packaging owner checks the zip size.
- The reload finish stage is the legacy `Reload` bank. Per-class stage speeds: Pistol 1.08, Smg 1.04, Rifle 1.00, Marksman 0.95, Shotgun 0.92, MachineGun 0.90.

**`ENEMY_VOICES: &[(&str, EnemyVoice, speed, gain_db)]`.** Presentation-only; no content change.

| archetype_id | Voice | Speed | Gain |
|---|---|---|---|
| shambler | Shambler | 1.00 | |
| hungry-shambler | Shambler | 0.94 | |
| runner | Runner | 1.00 | |
| plated-runner | Runner | 0.95 | |
| caller-pack | Runner | 1.04 | |
| skitter | Swarm | 1.00 | -2 dB |
| brute | Brute | 1.00 | |
| breacher-pack | Brute | 0.97 | |
| armored | Armored | 1.00 | |
| demolisher | Demolisher | 1.00 | |
| siege-pack | Demolisher | 0.96 | |

- Provisional goal-6 rows, added by whoever appends the ids: exploder→Exploder; splitter→Brute 0.90; spitter→Spitter; shield-bearer→Armored 0.92; sprinter-swarm→Swarm 1.08; juggernaut→Demolisher 0.85 at +1 dB.
- An unknown id falls back by EnemyKind. Elites play their family's bank at speed x0.94 and +1 dB (no files).
- `BOSS_VOICES` gives one bank per BossKind. The capstone plays its kind's bank at speed 0.94 and +1 dB. Legacy `Cue::Boss` and `Cue::Kill` stay as fallbacks for `boss_kind None` and unresolved deaths.

## 3. Loudness, mastering and the gate

**Mastering (`synth/master.rs`).** Every Rust family:
1. is normalised to **-21.0 +/-0.5 LUFS momentary-max** (mono file);
2. then passes a deterministic 4x-oversampled lookahead true-peak limiter: ceiling **0.80 FS**, lookahead 1.5 ms, release 40 ms;
3. has its gain reduction asserted **<= 5 dB** per file and recorded as `limiter_gr_db`.

**Runtime balance (`mix.rs`)** keeps today's in-game relative levels, clamped to [-12, 0] dB. Top row 0 dB = 1.0.

| Cue | Gain (dB) |
|---|---|
| Shotgun, Throw, Explosion, Boss, Structure, Trap, Defeat, BossArrival, BossDeath | 0 |
| Marksman | -1.2 |
| MachineGun | -1.3 |
| BossPhase | -1 |
| Damage | -1.7 |
| Rifle | -2.0 |
| Pistol | -2.1 |
| Melee | -2.3 |
| Victory | -2.7 |
| Molotov | -3.4 |
| Retreat | -4.0 |
| Fire | -4.4 |
| Wave | -4.5 |
| TurretShot | -4.6 |
| Smg | -4.7 |
| Reload | -5.5 |
| Windup | -6 |
| LootDrop, LootReveal | -6 |
| Impact | -6.1 |
| Kill, EnemyDeath | -6.9 |
| Contact | -7 |
| Armor | -7.1 |
| ReloadOut, ReloadIn, ShellInsert, BoltCycle | -7.5 |
| Block | -10 |
| CoinBatch | -8 |
| Ready, Confirm, Ability, BossExposed | -9.8 (BossExposed is Ready +2 dB) |
| Incoming | -11.1 |
| DryFire | -12 |

- Victory, Defeat and Retreat move to the Music bus with a +1.2 dB compensation.
- One calibration knob, `MIX_CALIBRATION_DB`, applies to the Effects, UI and Music buses and replaces the hidden x0.7. Its initial value is -1.4 dB (0.85). `AMBIENCE_CALIBRATION_DB` handles loops. Both are tuned in 0.5 dB steps against the render.

**Slider defaults** (constants in `horde_audio::mix`; Settings::default() uses them, pinned by a parity test):

| Slider | Old | New |
|---|---|---|
| master | 0.6 | 0.8 |
| effects | 0.7 | 0.8 |
| ambience | 0.15 | 0.5 |
| ui_volume | new | 0.7 |
| music_volume | new | 0.7 |

**Headroom proofs.** The old `audio.rs:477` six-voice assert is retired and replaced by:
- `single_voice_peak_at_max_sliders_le_0_75`: 0.80 x row x calibration x jitter (+0.75 dB) x pan near-gain (1.0) <= 0.75. Worst case today: 0.80 x 1.0 x 0.85 x 1.09 = 0.74.
- `atmosphere_peak_at_max_sliders_le_0_57`: the sum of the normalised loop weights x the provenance loop peaks.
- The measured gate.

If calibration needs more than -1.4 dB, compress the loud rare rows first; never relax the bound.

**Gate (`render.rs`, `tests/offline_mix.rs`).** The render runs the real `event_requests` → `Mixer` → `dsp` path at the default slider constants, metered by `meter.rs` (BS.1770-4: K-weighting derived per rate, 400 ms blocks with 75% overlap, -70 LUFS absolute and -10 LU relative gates, 4x true peak). Scenes:
- `reference_mixed_60s`:
  - 8 s Camp with two Confirm cues, then 44 s Battle (seed 1, Mode::Personal, deploy(0), `horde_core::sim::smoke_input`, one frame per tick, as in `determinism.rs:59-77`), then 8 s Results (stinger, LootReveal, CoinBatch, Paused ambience context);
  - rendered twice: stage-1 theme ambience and a rain theme;
  - each must read **-25 <= I <= -21 LUFS and TP <= 0.95**.
- `dense_battle_60s`: `horde_core::sim::maximum_progression_render_fixture()` with smoke_input. It must hold TP <= 0.95 and short-term max <= -14 LUFS; the integrated value is logged.
- `max_sliders_60s`: logged only. If its sample peak > 1.0, file a follow-up.
- A non-ignored `render_10s_is_finite_and_audible` runs in normal `cargo test`.
- Escalation if a default render has TP > 0.95, in order: lower the caps, then a stronger density curve, then a runtime master limiter (custom mixer source). The limiter is user-permitted only when the measured TP > 0.95.

## 4. Pan (decision 17)

**Mapping (B).** Along the lane, relative to the listener.
- `p = clamp((x - DEFENDER_X) / (width_yards - DEFENDER_X), 0, 1)` with `DEFENDER_X = 0.5` (`sim.rs:518-519`).
- x comes from the event position: Strike.from/to, Kill.at, Spawn.at, Windup.from, Explosion.at, LootEvent.at.
- It is zoom-independent under the user's uniform zoom-to-fit camera, because screen x is linear in yards, and it never reads the camera rect.
- Positional cues: EnemyWindup, EnemyContact, EnemyDeath, Kill, Impact, Armor, Block, Explosion, Molotov, Fire, Trap, TurretShot, Incoming, Structure, LootDrop.
- Centred: the defender's own gun, reload stages, DryFire, Damage, Melee, Throw, UI, stingers, Wave, BossWarning, boss arrival, music and ambience.

**Law (A, peak-safe balance).**
- Right channel 1.0; left channel `1 - D*p`.
- D = 0.30 normally, 0.15 in reduced effects, 0 with `Settings.mono_audio`.
- At p = 1: ILD 3.1 dB, mono fold-down -1.38 dB, power -1.28 dB.
- Centre equals today's dual-mono level, so calibration does not depend on pan, and pan never raises TP.

**Worker observer window** (`ui/operations.rs:277`): centred. Observed shots use `Tone::DISTANT` (speed 0.97, low-pass 3500 Hz, -4 dB). There are no reload or dry-fire diffs for expeditions, and `runtime.rs:441-453` audibility is unchanged (with a 3 kHz "radio" low-pass on alerts).

## 5. Voice implementation

**`PannedClip`** is a Decodable registered with `add_audio_source::<PannedClip>()`.
- Fields: `{ source: AudioSource /*Arc bytes, zero copy*/, gains: [f32; 2], lowpass_hz: Option<f32>, control: Arc<VoiceControl> }`.
- The decoder wraps `<AudioSource as Decodable>::Decoder` and yields interleaved stereo f32 at the source rate.
- It ramps towards `VoiceControl.target_gain` (`AtomicU32` f32 bits, read every 64 samples, one-pole tau 8 ms).
- It ends cleanly when `stop` is set and the gain reaches 0.
- Speed and jitter come from `PlaybackSettings.speed`. Bus gains use the existing `sink.set_volume` pattern (`sfx.rs:199-221`).

**Fallback (A).** If the variant's `Handle<AudioSource>` is not yet loaded, the voice spawns as plain `AudioPlayer<AudioSource>` (centred, unfiltered), so no cue is lost.

**Ambience loops** (22.05 kHz) stay plain AudioSource loops, dual mono. Their dual-mono contribution is modelled in the renderer.

## 6. Mixer policy

**Start budget per frame:** Effects 3, UI 2, Music 1, plus a reserved player-weapon lane. The root defender's own shot always starts if its gate passes.

**Selection** keeps today's policy as data:
- `SUBSUMES = [(Wave, Incoming), (Kill, Impact), (EnemyDeath, Impact), (Defeat, Structure), (Victory, Wave)]`;
- sort by `(priority, insertion)` and dedup by `(cue, bank)`;
- critical cues (priority <= 3) go first, then one non-fire cue with priority <= 8, then fire rotated by a presentation counter.

**Gate groups:** Death 160 ms (shared by Kill and every EnemyDeath), Windup 300, Contact 260, BossArrival 3000, CoinBatch 600, DryFire 350.

**Caps:**
- Bus: Effects 8, UI 3, Music 1 stinger + 2 track voices, Atmosphere 6 loops.
- Family (`max_concurrent`): player guns 2, turret 1, EnemyWindup 2, EnemyContact 2, death group 2, Impact 2, Armor/Block 1, Explosion 2, Molotov 1, Fire 1, reload-stage group 1, radio (Wave, Incoming) 1, each boss cue 1, LootDrop 1.

**Stealing:**
- At a family cap, steal that family's oldest voice that is no more important than the start (priority value >= the start's), and refuse the start when every member is more important. Only the radio family mixes priorities: an Incoming squelch (12) never cuts a sounding Wave warning (1), while a Wave still replaces an Incoming (closing phase).
- At a bus cap, steal the highest priority value (least important), ties broken by oldest `started_ms`. Equal-priority steals are allowed only for fire and impact families.
- The victim gets a 40 ms fade through VoiceControl and then stops. Fading voices do not count toward caps.
- UI and Music voices are never stolen by Effects.

**Round-robin:**
- Per-bank `RoundRobin { last: u8 /*u8::MAX = none, also after load*/ }`, with `next = (last + 1 + h % (n-1)) % n` and `h = splitmix64(((bank_ordinal as u64) << 32) ^ bank_counter)`.
- This guarantees no immediate repeat for n >= 2.
- No Game or core RNG is reachable.

**Jitter** (from other bits of h; kept in reduced mode, because it adds no intensity):

| Group | Speed | Gain |
|---|---|---|
| Guns | +/-2% | +/-0.75 dB |
| Enemy vocals | +/-4% | +/-1 dB |
| Impacts and deaths | +/-3% | +/-1 dB |
| Boss | +/-1% | 0 |
| UI | 0 | +/-0.3 dB |
| Stingers | none | none |

**Ducking** (`mix::DUCKS`, dt-based like `MixEnvelope`, `atmosphere_audio.rs:205-229`):

| Trigger | Duck | Attack | Hold | Release |
|---|---|---|---|---|
| Victory, Defeat, Retreat stingers | Atmosphere -6 dB, Music tracks -9 dB | 40 ms | clip length | 600 ms |
| BossArrival / legacy Boss | Atmosphere -6, Music -9 | 30 ms | 1200 ms | 800 ms |
| BossPhase | Atmosphere -4 | 30 ms | 600 ms | 500 ms |
| Wave | Atmosphere -4 | | 600 ms | 400 ms |
| Explosion | Atmosphere -3 | | 300 ms | 400 ms |
| LootReveal, CoinBatch | Atmosphere -3 | | 400 ms | 400 ms |

- The summed duck per bus is floored at -9 dB.
- **Effects never duck** (graft): this protects late-battle warnings.

**Density:**
- Non-warning, non-player-gun Effects voices get `min(1, sqrt(4/N))` for N live effect voices (attack 10 ms, release 250 ms).
- Atmosphere gets `-4 dB x clamp((N-2)/4, 0, 1)` with a 300 ms release.

**Reduced effects** softens and never removes information:
- per-cue `reduced_gain_db` (default -3.7) and `reduced_cooldown_x` (default 1.5), asserted for every cue;
- pan depth 0.15;
- stingers -3 dB;
- DryFire, BoltCycle, ShellInsert and LootDrop all stay, at reduced gain and longer cooldown.

## 7. Event mapping (`horde_audio::events::event_requests`, exhaustive, no wildcard)

**Combat events:**
- **Shot:** an Actor::Defender shot plays its class family, whatever its Mode (removes `runtime.rs:390-392`), with the weapon taken from `attempt.profile`. This makes takeover recruits correct. The Turret actor plays TurretShot; trap strikes play Trap. Actor::Enemy, Ordnance and Unknown are explicitly silent.
- **Quality tone** (`QUALITY_TONE`) uses `attempt.profile.equipment_visual.ranged.quality`:

  | Quality | Speed | Low-pass | Gain |
  |---|---|---|---|
  | Poor | 0.985 | 6000 Hz | -0.5 dB |
  | Common | 1.000 | 9000 Hz | 0 |
  | Uncommon | 1.000 | off | 0 |
  | Rare | 1.005 | off | +0.3 dB |
  | Epic | 1.010 | off | +0.5 dB |
  | Legendary | 1.015 | off | +0.7 dB |

  An unequipped ranged slot counts as Common.
- **Hit, WeakPoint:** Impact. **Armor:** Armor.
- **Defended:** Block (Armor bank, speed 0.85, -3 dB) when the defender defended; Trap for trap strikes.
- **Miss:** silent; the tracer is the twin.
- **Damage:** Damage. **Melee:** Melee. **Throw:** Throw.
- **Explosion:** Molotov or Explosion. **Burn:** silent per event; the fire-zone bed at `runtime.rs:458-471` covers it.

**Enemy and boss events:**
- **Kill:** EnemyDeath through `EnemyVoiceIndex`. If the index misses, use the `CorpseRecord` with the same id (kind, boss_kind). A boss Kill plays BossDeath for its kind. With no resolution at all, play legacy Kill.
- **Spawn:** Incoming. **Boss:** BossArrival for its BossKind (capstone: speed 0.94, +1 dB); `boss_kind None` plays legacy Boss.
- **StructureLost:** Structure.
- **Contact:** EnemyContact, for defender and structure contact.
- **Windup:** EnemyWindup **only when `structure_id == None`** (defender-targeted). Structure windups stay visual-only, using the ring at `art/battlefield/enemies.rs:129-140`.
- **BossWarning, Reinforcements:** Wave.
- **BossExposure:** `exposed: true` plays BossExposed (Ready bank, 0.9, +2 dB). `false` is silent; this fixes `runtime.rs:376`.
- **BossPhase** (added by enemy-balance in wave 2): BossPhase for its kind. Any later variant is a compile error until it is mapped.

**Other events:**
- **Ability:** Ability (Ready bank, 0.92). **CooldownReady:** Ready. **RuleResolved:** silent.
- **AttemptFinished:** Victory, Defeat or Retreat on the Music bus.

**EnemyVoiceIndex** is presentation-only, keyed by `(BattleKey, enemy_id) -> EnemyVoiceRef { voice, speed, gain_db, elite, boss: Option<BossKind> }`.
- `sync(&Attempt, BattleKey)` runs in `run_tick` before `game.tick(input)` and again inside `tick_sounds`.
- It clears and rebuilds from `attempt.enemies` when the attempt id or battle changes (load, deploy, observed expedition). Otherwise it adds ids not yet indexed.
- Entries are removed on Kill.
- A same-tick spawn-and-kill misses the index and resolves through the CorpseRecord kind.

**GunTracker** (per BattleKey, observed after every `run_tick` with that tick's Input):
- It primes on a new attempt id, so a load never bursts cues.
- `reload_left` 0 → >0 plays ReloadOut and stores the total. Crossing total/2 plays ReloadIn.
- Shotgun: ShellInsert at k/(n+1) of the total for `n = min(magazine - ammo, 6)`.
- `SimEvent::Reload` plays the legacy Reload as the finish stage.
- The silent one-turn NPC refill (ammo rises to the magazine with no Reload event, `npc.rs:101-111`) also plays Reload.
- Marksman: BoltCycle 300 ms after each shot.
- DryFire plays on the `input.fire` rising edge, and at most every 350 ms while held, when Mode::Personal && ammo == 0 && reload_left == 0 && melee_commit == 0 && !auto_reload (mirrors `manual.rs:101-108`).
- No core events, so no event_seq move.

**LootTracker** reads new `attempt.loot_events` after each tick, tracking the last `(tick, item_id)` seen per battle.
- A Rare+ drop plays LootDrop (Effects, panned by `LootEvent.at`, tier speed Rare 1.00 / Epic 1.03 / Legendary 1.06).
- The widget emissions at `ui/loot.rs:150`, `ui/context.rs:254-256` and `ui/operations.rs:303` are removed.

**Results and commands.**
- A presentation-local `ResultsAudioGuard { last_attempt: Option<u64> }`, keyed by `report.attempt_id`, fires once on the first Results frame:
  - LootReveal (UI, delay 150 ms, speed from `quality_speed(best)`, +1 dB per tier above Rare) when the report has loot;
  - then exactly one CoinBatch (delay 450 ms) when `report.auto_sold` is non-empty.
- `SellItem`, `sell_extras` and apply-rule-to-stash each play exactly one CoinBatch per successful command with sold >= 1, in the runner apply step beside `confirms_purchase` (`runner/apply.rs:84-117`).
- Each expedition notice whose receipt sold >= 1 item plays one CoinBatch.
- No CoinBatch is produced while on `Screen::Battle`, and loot cues take no auto-sell input.
- No hover cue, enforced by a guard test.

## 8. Buses, settings and sliders

**Buses:**
- **UI:** Confirm, LootReveal, CoinBatch.
- **Music:** Victory, Defeat, Retreat stingers plus the track slots.
- **Effects:** every combat cue, warnings included. No slider other than Master or Effects can mute a danger cue.
- **Atmosphere:** loops.

**Settings** (settings.json, not the game save; names frozen in wave 1):
- `music_volume: f32 = 0.7`, `ui_volume: f32 = 0.7`, `mono_audio: bool = false`.
- `#[serde(default)] audio_mix_version: u8`, field-level, so a missing field reads 0.
- `const CURRENT_AUDIO_MIX: u8`: 0 in wave 1 (fields only, values unchanged), bumped to 1 in wave 6 with the new defaults.
- `Settings::load`: if `audio_mix_version < 1` and (master, effects, ambience) are within 1e-6 of (0.6, 0.7, 0.15), set them to (0.8, 0.8, 0.5). Then always set the version to `CURRENT_AUDIO_MIX`. Customised values are kept.
- `bindings: [String; 8]` is untouched.

**Sliders** (`ui/preferences.rs:96-101`): Master, Combat effects, Interface, "Music & stingers" with the caption "Music tracks are not in this build; this sets victory, defeat and retreat stingers.", Atmosphere, and the toggle "Mono audio (no stereo pan)". The new sliders stay hidden until wave 6 wires the buses.

## 9. Music (user decision)

- `MusicDirector` maps `MusicInput { screen, battle: Option<{enemies_alive, nearest_x_frac, hp_frac, boss_alive, paused}> }` to `MusicState { Silent, Menu, Camp, BattleLow, BattleHigh, Boss, Results }`. Briefing maps to Camp; Results and Upgrades map to Results.
- `intensity = 0.45*min(1, enemies/40) + 0.35*(1 - nearest_x_frac) + 0.2*(1 - hp_frac)`.
- BattleHigh starts at >= 0.60 held 2 s and returns to BattleLow at <= 0.35 held 4 s. Boss is immediate while a boss is alive. Paused uses context gain 0.35.
- `const TRACKS: [(MusicState, Track); 7]`. Every slot is `Track::Deferred("music tracks deferred: no new music files (decision 9, user 2026-11-04)")`.
- `Track::File` playback with a 2 s smoothstep crossfade is implemented and tested on the envelope only.
- The user's chat decision is the approval record. FU-AUD-MUSIC ("7 track slots deferred, no files, no AI music") goes in the plan's follow-ups.
- The director state is shown in the F3/review report.

## 10. Provenance and generators (wave 1)

- **Archive.** The 12 legacy WAVs (armor, boss, damage, incoming, melee, npc_shot, ready, reload, shot, structure, trap, turret_shot) move to `archive/audio/`, with `archive/audio/PROVENANCE.json` holding their original records and preserved_originals verbatim.
- **v1 records.** `archive/audio/PROVENANCE.v1.json` and `archive/audio/SURVIVAL_PROVENANCE.v1.json` keep the v1 files verbatim (72 survival hashes).
- **`assets/audio/PROVENANCE.json` schema 2 (merged; SURVIVAL_PROVENANCE.json removed):**
  - top level: `{schema: 2, license, policy: {recordings: false, third_party: false, ai_audio: false, music: false}}`;
  - per file: `{file, bank, variant, generator, generator_sha256, seed, sample_rate, channels, frames, sha256, peak, true_peak, mmax_lufs, target_mmax_lufs|null, limiter_gr_db|null, seam_limit|null}`.
- **generator_sha256** is the LF-normalised sha256 over the sorted generator file set. This stops CRLF checkouts from flapping. The recorded `50793b...` hash is stale today.
- `assets/CREDITS.md:40-53,90-95` are rewritten. `archive/` is never packaged (`package_playtest.ps1:26`).
- **Python scripts:**
  - output maps: ambience and loops go to `assets/audio`; legacy cues go to `archive/audio`;
  - owned-file iteration only, replacing the glob at `generate_atmosphere_audio.py:151`;
  - per-file rate assert (:134);
  - originals are read from their new homes (:144);
  - merge-write only the script's own records (:164);
  - **the default mode is `--check`** (render in memory, compare sha256, write nothing, non-zero exit on mismatch, per-file report); `--write` is explicit;
  - both `--check` runs join the full verify.
- **Audition reel.** It goes to `target/audio-audition/` and is not committed. `docs/audio/survival-audition.{wav,csv}` (2.18 MB) move to `archive/audio/audition/`.

## 11. Decisions table

| Question | Chosen | Rejected (reason) |
|---|---|---|
| Archetype for Kill, Windup, Contact, Boss | EnemyVoiceIndex, with CorpseRecord kind fallback | SimEvent EnemyRef via `typed()` (enemy already removed at combat.rs:322; core API carries presentation data); CorpseRecord.archetype_id (saved field and GOLDEN move for audio) |
| Code location | `crates/horde_audio`, std-only | cfg(test) module in horde_game (every audio test builds Bevy; kept as the fallback only); example binary in horde_game (builds Bevy) |
| Loudness | -21 M-max / 0.80 TP mastering + calibration knob + new defaults, gated by render | Master limiter from the start (user rule); file lift with old defaults (6-8 dB of limiting); -18 M-max at 0.891 (the single-voice bound fails without a bigger cut) |
| Headroom proof | Single-voice and atmosphere bounds + measured gate | Static 6- or 8-voice sum <= 0.95 (forces about -35 LUFS) |
| Pan mapping | Along-lane, listener-relative | Lane-index L/R (lanes are vertical rows); screen-centred x (defender gun hard left); x/y blend |
| Pan law | Peak-safe balance, D 0.30 | Equal-power (-3 dB centre needs compensation); rodio Spatial (farther ear louder, 1/d^2) |
| Voice | PannedClip Decodable + AudioSource fallback | Custom mixer (audio-thread risk; escalation only); frame-step sink fades (zipper) |
| Message | `SoundRequest` with `PartialEq<Cue>` | Side CuePlacement stream; data-carrying Cue variants (break `cue as usize`) |
| Enemy banks | 8 voices x 3 stages x 5 + id table | Per-archetype banks (about 380 files); audio field in enemy_catalog.json (presentation in core) |
| Boss banks | 3 BossKind x 3 stages x 3; capstone by speed/gain | Capstone as a 4th bank kind (decision 12: no new BossKinds) |
| Reload / dry fire | Presentation GunTracker | Core ReloadStarted/DryFire events (event_seq and trace move); npc.rs bugfix (not needed) |
| Gun granularity | Shared stages + shotgun ShellInsert + marksman BoltCycle (19 files) | Per-class stages (120 files); separate ReloadCharge family (redundant with the Reload bank) |
| NpcShot | Retired row, files to `archive/audio/retired/` | Camp Murmur (behaviour beyond the brief); keep for NPC shots (goal 8); ship orphans with an allow-list |
| Windups audible | Defender-targeted only | All windups (constant noise at 225 enemies); boss only |
| Ducking | Atmosphere and Music only; Effects never duck | Effects -3 dB on stingers (masks warnings); density-driven atmosphere-only duck alone |
| Caps | Effects 8 / 3 starts + player lane | Keep 6 / 2 (enemy banks starve guns) |
| Reduced mode | Softens, never removes (DryFire, Bolt, LootDrop kept) | Disable DryFire/Bolt; LootDrop Epic+ only |
| Loot cue source | Runner LootTracker + Results guard | Widget-draw cue (widget IO); settlement cue (fires before Results) |
| Coin cue | One CoinBatch per batch or command | Per-item cue; SimEvent::AutoSold (event_seq churn) |
| Provenance | One schema-2 PROVENANCE.json + v1 archives | Two files (two schemas forever) |
| Python | Fix in place, default --check | Port loops to Rust (hashes change); archive scripts |
| Settings names | `music_volume`, `ui_volume`, `mono_audio`, `audio_mix_version` | `ui` (ambiguous); `music` (no audit uses it; inconsistent with ui_volume) |
| Settings wave | Names in wave 1, values in wave 6 | All in wave 6 (misses the freeze); values in wave 1 (louder before mastering) |
| Music | Director + Deferred slots + Music-bus stingers | Ambience-derived beds; procedural music (decision 9) |
| Liberation stinger | Not added | Depends on a report field another remit owns; Victory covers it |
| Visual twins | Per-row test asserting cue + view-model element | Metadata-only list |

## 12. Steps (each is one commit; failing test first)

**Wave 1**
1. **Create the `crates/horde_audio` skeleton** (`sha256.rs`, `wav.rs`, `provenance.rs`).
   - Run `cargo update --workspace --offline`; the Cargo.lock diff must only add the path package.
   - Failing first: `sha256_matches_fips_180_4_vectors` ("abc" and the empty string); `cargo tree -p horde_audio` contains no bevy.
2. **Archive move + merged schema 2.**
   - Tests live in `crates/horde_game/src/audio.rs` for now and move to horde_audio in step 6.1.
   - Failing first: `no_orphan_wavs_in_shipped_audio` (12 orphans today); `every_shipped_wav_has_schema2_provenance_with_matching_sha256` (no generator or seed); `archived_audio_keeps_original_sha256`.
   - Done when assets/audio holds exactly 78 WAVs.
3. **Python generators.**
   - Failing first: `python scripts/generate_atmosphere_audio.py --check` trips the assert at :134 (on a scratch copy of the tree); `generate_audio.py --check` is an unknown mode.
   - Done when both exit 0 with git status clean.
4. **Settings field freeze.**
   - `old_13_field_settings_file_loads_with_audio_defaults` (custom bindings kept);
   - `missing_audio_mix_version_reads_zero`;
   - `migrate_audio_mix_is_noop_at_version_0_constant`.

**Wave 2 (dependency, owned by enemy-balance).** `SimEvent::BossPhase { boss_id, kind, phase: u8, lane, at }`, inside wave 2's GOLDEN/TRACE re-bless with save-determinism's reason. Loot-economy does not add SimEvent::AutoSold.

**Wave 5 (dependency, owned by vfx).** Boss-phase, BossExposed, spitter and exploder tells exist in BattleViewModel.

**Wave 6**
- **6.1 Move Cue, banks and legacy synthesis into horde_audio, verbatim.**
  - `raw_synthesis_matches_v1_library`: raw v0-2 at 0.18 peak PCM16 matches the `SURVIVAL_PROVENANCE.v1.json` sha256s.
  - `legacy_family_order_is_frozen`, `bank_ordinals_are_append_only`.
  - GOLDEN and TRACE stay unchanged.
- **6.2 Mix table.**
  - The first commit is value-preserving (old numbers, x0.7 expressed as `MIX_CALIBRATION_DB = -3.1`). Value changes come in later commits.
  - `every_cue_has_one_mix_row_bank_and_bus`;
  - `horde_game_audio_sources_define_no_gain_tables`, a scan for `fn gain(`, `fn cooldown_ms(`, `fn priority(` and `* 0.7` (fails today at audio.rs:86/101 and sfx.rs:84);
  - `every_cue_reduced_gain_below_normal_and_cooldown_not_shorter`;
  - `frequent_cues_have_at_least_five_variants`;
  - `every_warning_row_names_a_visual_twin`.
- **6.3 Meter.**
  - `ebu3341_case1_stereo_1khz_minus23` (+/-0.1);
  - `ebu3341_case2_minus33`;
  - `ebu3341_gating_minus36_minus23_minus36_reads_minus23`;
  - `silence_reads_neg_inf`;
  - `true_peak_of_fs_over_4_sine_at_45deg_is_unity` (+/-0.05);
  - `momentary_max_matches_python_baseline_survival_pistol_0` (-35.9 +/-0.3).
- **6.4 Renderer + BEFORE numbers.**
  - A FrameActions shim of today's policy (x0.7, 6 voices, 2 starts, rotation, dual mono) on today's assets.
  - The ignored gate tests run red (about -38 to -42 LUFS predicted). Numbers are recorded in the plan.
  - Parity test: `SceneAmbience::from_theme(stage1) == render::REFERENCE_WEIGHTS`.
- **6.5 Exporter v2.**
  - `every_rust_family_masters_to_minus21_mmax_within_0_5_tp_le_0_80_gr_le_5db`;
  - `legacy_v0_2_master_input_equals_v1_raw`;
  - `new_variants_differ_from_neighbours`;
  - `ui_and_stinger_synthesis_is_non_melodic` (no spectral peak > 12 dB above its 1/3-octave neighbourhood for > 150 ms);
  - `generator_sha256_matches_lf_normalised_source`;
  - `export_check_is_idempotent`;
  - NpcShot files move to `archive/audio/retired/`.
- **6.6 Buses + routing + sliders.**
  - `ui_cues_follow_ui_volume_not_effects`;
  - `stingers_follow_music_bus_and_warnings_do_not`;
  - `each_bus_slider_changes_only_its_voices` (App test, like sfx.rs:289-346).
- **6.7 PannedClip + pan + fallback.**
  - `pan_is_mono_compatible_for_every_lane_count_3_to_17` (<= 1.5 dB, never > 1.0);
  - `pan_depends_only_on_x_and_width`;
  - `defender_ui_stinger_ambience_centred`;
  - `mono_audio_forces_centre`;
  - `panned_clip_outputs_two_channels_at_source_rate`;
  - `unloaded_clip_falls_back_to_audio_source_player`;
  - `single_voice_peak_at_max_sliders_le_0_75`;
  - `atmosphere_peak_at_max_sliders_le_0_57`.
- **6.8 Selection and budgets.** `player_shot_starts_even_with_two_critical_cues_in_frame`. The old sfx.rs:228-288 cases are re-expressed.
- **6.9 Caps and stealing.**
  - `family_cap_steals_own_oldest`;
  - `incoming_never_steals_a_sounding_wave`;
  - `steal_prefers_oldest_among_equal_priority_and_fades_40ms` (dt-independent at 30 and 144 FPS);
  - `ui_and_music_never_stolen_by_effects`;
  - `voice_control_fade_has_no_step_over_0_1`.
- **6.10 Round-robin and jitter.**
  - `round_robin_never_repeats_in_1000_triggers` (every bank at its gate cadence, interleaved; each variant used >= 0.5*1000/n times);
  - end-to-end with 1,000 Smg requests at 55 ms;
  - `jitter_bounded`;
  - `plans_independent_of_game_seed`;
  - `horde_audio_never_reads_core_rng` (structural scan).
- **6.11 Ducking and density.**
  - `boss_arrival_ducks_atmosphere_6db_releases_800ms`;
  - `effects_bus_never_ducks`;
  - `ducking_is_display_rate_independent`;
  - `density_compensation_caps_effects_rms_growth_and_exempts_warnings`.
- **6.12 Exhaustive mapping + EnemyVoiceIndex.**
  - `sim_event_mapping_is_exhaustive` (25 variants plus BossPhase);
  - `boss_exposure_closing_is_silent`;
  - `npc_defender_shotgun_plays_shotgun_family`;
  - `takeover_recruit_rare_shotgun_gets_rare_tone`;
  - `observed_expedition_shot_uses_distant_tone`;
  - `windup_cue_only_for_defender_targeted_windups`;
  - `every_catalog_archetype_has_explicit_voice_row_with_existing_files`;
  - `every_boss_kind_has_arrival_phase_death_banks_and_capstone_uses_kind_bank`;
  - `kill_after_load_resolves_from_rebuilt_index`;
  - `same_tick_spawn_kill_resolves_from_corpse_kind`;
  - `death_cues_share_kill_gate_and_subsume_impact`;
  - per-row `warning_twin_<cue>` tests: build the state and assert the cue and the view element (e.g. `EnemyView.windup.is_some()`, `battle_view.rs:817`).
- **6.13 GunTracker.**
  - `manual_reload_plays_out_in_finish_once`;
  - `shotgun_reload_plays_n_shell_inserts_capped_6`;
  - `dry_trigger_auto_reload_off_plays_dry_fire_once_per_350ms`;
  - `npc_one_turn_refill_plays_reload`;
  - `loaded_mid_reload_emits_no_stage_burst`;
  - `stage_speed_follows_weapon_class`;
  - `reduced_effects_keeps_dry_fire_and_bolt_at_reduced_gain`;
  - all at speeds 1x and 4x.
- **6.14 UI cues.**
  - `fifty_auto_sold_items_one_coin_batch_on_results_none_on_rerender`;
  - `rule_off_or_zero_sold_no_coin_batch`;
  - `sell_extras_of_50_plays_one_coin_batch`;
  - `two_worker_batches_at_most_one_each`;
  - `no_sale_cue_in_battle`;
  - `rare_drop_cue_comes_from_runner_not_widget`;
  - `loot_reveal_follows_quality_speed_and_ignores_auto_sell_rule`;
  - `no_widget_hover_writes_a_sound_request`: hover every interactive rect on Results, Upgrades, Camp home, Equipment and Settings at 1440x900 and 1050x700@1.35. This is a guard; a mutation proof goes in the commit message.
- **6.15 Music.**
  - `music_director_matrix` (with hysteresis);
  - `every_music_state_resolves_to_file_or_deferred`;
  - `every_stinger_resolves_to_existing_bank`;
  - `music_volume_scales_stingers_only`.
- **6.16 Settings v1.**
  - Set `CURRENT_AUDIO_MIX = 1` and switch Settings::default() to the new defaults.
  - `untouched_old_defaults_migrate_once`;
  - `customised_levels_are_kept`;
  - `settings_defaults_match_horde_audio_constants`;
  - `old_default_f32_roundtrip_is_exact`.
- **6.17 Calibrate.**
  - The ignored gate tests turn green: both reference themes at -23 +/-2 / TP <= 0.95; dense TP <= 0.95 with short-term max <= -14; max-slider numbers logged.
  - The before/after table goes in the plan. Run `--smoke`. GOLDEN and TRACE stay unchanged.
- **6.18 Optional cue-log replay.**
  - The review service writes `target/review/<run>/cues.jsonl` (not a widget); `replay_cue_log` renders it.
  - `cue_log_round_trips_frame_actions`. The target is within 1 LU of `reference_mixed_60s`, or the delta is explained. This brings the gate closer to a real "mixed capture".

**Wave 7.** Docs and packaging.
- Rewrite `docs/AUDIO_SURVIVAL.md`: buses, table, pan, banks, exporter route replacing :42-49, headroom text at :38, gate numbers.
- `docs/PLAYTEST_GUIDE.md:132-144`: listening pass PENDING with device, sliders and mono fields; reel link replaced.
- `docs/ACCEPTANCE.md`, `docs/VALIDATION.md`, CREDITS.
- `package_playtest.ps1:26,33`: no archive/ and no audition in the package.
- Failing first: `check_views.py` link check, with no `docs/audio/survival-audition.*` links left (fails today at AUDIO_SURVIVAL.md:5 and PLAYTEST_GUIDE.md:142).

## 13. Digest moves

None from audio.
- Wave 6 touches no horde_core file. GOLDEN and GAMEPLAY_TRACE must be byte-identical in every wave-6 commit (`cargo test --locked -p horde_core --test determinism`).
- BossPhase moves the event_seq and trace in wave 2. That move is owned by enemy-balance and re-blessed by save-determinism.
- Rejected on digest grounds: CorpseRecord.archetype_id, core ReloadStarted/DryFire, an npc.rs Reload emit, and SimEvent::AutoSold.

## 14. Risks

- The loudness numbers come from a model. The real render may differ by +/-3 dB.
  - Mitigation: the single calibration knob and the measured gate.
  - Caps of 8 voices / 3 starts and the density compensator were not in the model.
- Mastering changes every survival hash and the transient character (up to about 4.7 dB of gain reduction on Smg and Ready). A human listening pass is PENDING.
- The max-slider mix is ungated (logged only). rodio has no master insert.
- The new crate means a one-time lock update. The first `--locked` build fails until `cargo update --workspace --offline` runs.
- Shipped audio grows to about 281 WAVs (about 17-19 MB), plus about 8 MB of memory at preload.
- PannedClip depends on the bevy_audio 0.20 Decodable/Source signatures (rodio 0.22 NonZero channel and rate). The fallback keeps cues audible.
- Python `--check` depends on libm bit-exactness. Mismatches are reported per file and never auto-written.
- Goal-6 archetype ids are provisional. The coverage test couples enemy-balance appends to `bank.rs`, which is intended.
- Players with customised slider values keep them, but hear SFX about 10 dB louder relative to their saved ambience.
- The lane-pan interpretation is a judgement call. Only `mix::pan_for` and its tests change if the user disagrees.

## 15. Numbers to measure

- BEFORE and AFTER for `reference_mixed_60s` (two themes), `dense_battle_60s` and `max_sliders_60s`: integrated, short-term max, momentary max, sample peak and 4x true peak.
- Per-file `mmax_lufs`, `true_peak` and `limiter_gr_db` (the maximum must be <= 5 dB).
- Shipped WAV count and MB, and package zip size.
- Pan mono and power deviation at p = 1 (expected -1.38 / -1.28 dB).
- Round-robin distribution over 1,000 triggers.
- Frame-report Interface mean unchanged (<= 1.13 / 1.03 ms targets are owned by perf; audio systems are outside the egui metric). Audio-thread cost profiled once with the perf scene.
- The GOLDEN and TRACE hashes, unchanged across wave 6.

---

## Integration rulings

This section was added on 2026-11-05 when the synthesized spec above was copied into the plan. The spec above is kept verbatim as the design record. **Where this section and the spec disagree, this section wins.** The rulings come from four sources, applied in this precedence order: user decisions (U), orchestrator rulings (R, RL), master-planner reconciliations (P) and the five integration cluster rulings (C, D, E, S, W). The final saved-field table is `schema-v7.md`, the commit order is `waves.md` and the proof for every criterion is `acceptance.md`, all in this folder. The plan itself is `docs/plans/2026-11-05-production-pass.md`; any reference above to `docs/plans/2026-11-04-production-pass.md` means that file.

Each entry gives the binding ruling and then what it supersedes in **this** spec.

### User decisions

#### U2. Audio assets

- **Owner / wave:** audio, wave 6.
- **Ruling:** New gun, enemy and UI audio: YES (Rust offline exporter, 44.1 kHz). No new music files. No new ImageGen sheets.
- **Supersedes in this spec:** Any "no new audio" fallback; music tracks stay Deferred (see U4).

#### U3. Loudness gate (replaces the brief's -16..-14 LUFS)

- **Owner / wave:** audio, wave 6.
- **Ruling:** A 60 s mix at DEFAULT sliders reads -23 +/-2 LUFS integrated with true peak &lt;= 0.95 FS. Recorded in ACCEPTANCE.md, AUDIO_SURVIVAL.md and the plan (W7-03).
- **Supersedes in this spec:** Every reference to the brief's -16..-14 LUFS 60 s gate.

#### U4. MusicPlugin scope

- **Owner / wave:** audio, wave 6.
- **Ruling:** MusicPlugin = music director + Music bus + stingers from existing banks; every track slot is Track::Deferred; the slider is labelled "Music & stingers".
- **Supersedes in this spec:** Any track-file plan.

#### U7. Archive list

- **Owner / wave:** qa-captures, wave 1.
- **Ruling:** Archive to horde-defense/archive/: the 12 unreferenced WAVs, the 6 unreferenced root sheets in assets/art/, the 5 unreferenced animation strips in assets/art/animation/, pixel/keyart-v1.png, and survival_npc_shot_0..2.wav (Cue::NpcShot stays as a retired row). Nothing is deleted.
- **Supersedes in this spec:** The npc_shot needs_user item is answered yes; the location is archive/audio/ (not archive/audio/retired/, see S9/S10).

### Orchestrator rulings

#### R3. Audio mastering and limiter

- **Owner / wave:** audio, wave 6.
- **Ruling:** The exporter may apply an offline per-clip true-peak ceiling as asset mastering. A runtime master limiter is added only if the measured 60 s mix true peak exceeds 0.95 FS; the plan records whether it was needed.
- **Supersedes in this spec:** The unconditional 0.80 FS lookahead limiter as a runtime stage; mastering stays offline per clip.

#### R8. Single owners

- **Owner / wave:** progression-tree, wave 1-4.
- **Ruling:** Pacing harness v2 = progression-tree (wave 2; CD and EB add profiles on its API); per-archetype telemetry = enemy-balance; SHA-256 / provenance / orphan helpers and scripts/lib/Native.ps1 = qa-captures (wave 1; audio, vfx and perf reuse them); measure_performance.ps1 = perf-architecture on top of Native.ps1; storm tagging = vfx presentation table only (no world.json change); regional checkpoints are derived (no saved field), presentation lines by CD plus EB's Regional boss rank (HP and reward x1.2) in wave-4 balance, no theme-rule change; save_v7.json is cut at the wave-1 close and frozen, later cases use inline JSON or new fixtures; stress scene raises the staged enemy cap only behind a review/test-only seam (no production save accepts &gt; cap+1), owner perf.
- **Supersedes in this spec:** horde_audio sha256/provenance skeleton in wave 1.

#### R9. "50 sales produce 1 cue"

- **Owner / wave:** audio, wave 6.
- **Ruling:** One auto-sell or stash-apply batch of 50 items gives 1 cue; manual single sales may each cue.
- **Supersedes in this spec:** Any per-command reading that would require 50 single SellItem commands to give 1 cue.

### Master-planner reconciliations

#### P8. horde_audio crate

- **Owner / wave:** audio, wave 6.
- **Ruling:** No crate in wave 1. A Bevy-free path crate crates/horde_audio is proposed for wave 6 (A6-00) and is open for the user (default: create it). It adds no external dependency; the Cargo.lock diff is one path entry.
- **Supersedes in this spec:** Creating crates/horde_audio in wave 1 with sha256, wav and provenance.

### Cluster: settings-audio-tooling

#### S1. Settings v2 struct

- **Owner / wave:** save-determinism, wave 1.
- **Ruling:** Keep the 13 keys and bindings: [String; 8]. Add #[serde(default)] version: u32 (absent = 0), extra_bindings: ExtraBindings{world_clock "P", cycle_attack "Q"} (struct), window_mode: WindowModeSetting{Windowed, BorderlessFullscreen}, vsync true, low_latency false, music f32 ("Music & stingers"), ui f32 ("Interface sounds"), mono_audio false, quality_markers: QualityMarkers{ColorOnly, Text, Shape, TextAndShape}, palette: Palette{Standard, ColorBlind}, seen_callouts: Option&lt;BTreeSet&lt;String&gt;&gt;. Enums are append-only and safe under per-key salvage. SETTINGS_VERSION 1 from wave 1, 2 from wave 6. Defaults in settings::defaults (audio owns values; wave-1 0.6/0.7/0.15/0.7/0.7). Lands before perf's render_plan.
- **Supersedes in this spec:** music_volume / ui_volume / audio_mix_version.

#### S5. Audio slider defaults, mix version and mono

- **Owner / wave:** audio, wave 6.
- **Ruling:** No audio_mix_version: Settings.version is the mix version. Wave 6 sets SETTINGS_VERSION 2, switches defaults to calibrated values (targets master 0.8, effects 0.8, ambience 0.5, music 0.7, ui 0.7, retunable in 0.05 steps by the -23 LUFS gate) and enables the per-field migration against defaults::WAVE1_*. Audio wires mono_audio and the "Mono audio" checkbox in wave 6. UX's wave-3 audio section adds only the Music & stingers and Interface sounds sliders.
- **Supersedes in this spec:** 6.16 CURRENT_AUDIO_MIX / audio_mix_version; wave-1 "Settings field freeze" step (folded into SD's W1-31).

#### S7. SHA-256: one implementation

- **Owner / wave:** qa-captures, wave 1.
- **Ruling:** One std-only FIPS 180-4 SHA-256 at crates/horde_core/src/sha256.rs, #[cfg(any(test, feature = "test-support"))] pub mod sha256 with digest() and hex(). No new crate, dependency or Cargo.lock change. PowerShell uses Get-FileHash via Native.ps1 Get-FileSha256; Python keeps hashlib.
- **Supersedes in this spec:** sha256.rs in crates/horde_audio.

#### S8. Provenance and orphan audit helpers

- **Owner / wave:** qa-captures, wave 1.
- **Ruling:** One #[cfg(test)] mod asset_audit at crates/horde_game/src/asset_audit.rs (qa): workspace_root, memoised file_sha256, shipped_files, audio_manifests (both shipped manifests), art_manifests, archive_manifest. Runtime sets: visual_assets::loading::PATHS (pub(crate)), enemy_visuals sheets, player_models PNGs, #[cfg(test)] audio::runtime_wav_paths() (Cue::ALL minus retired x variants plus beds). vfx's provenance walker folds in; later owners extend only the runtime-set functions in the commit that adds files. Tests include every_shipped_wav_is_loaded_and_matches_provenance (78 then 75), shipped_art_has_provenance_with_matching_sha256, shipped_art_is_referenced_at_runtime, archived_files_keep_matching_provenance, player_model_png_sha256_matches_file, every_manifest_entry_exists.
- **Supersedes in this spec:** Tests "living in audio.rs for now".

#### S9. Archive layout

- **Owner / wave:** qa-captures, wave 1.
- **Ruling:** One qa commit with git mv mirrored under horde-defense/archive/: 6 root sheets to archive/art/, 5 strips to archive/art/animation/, pixel/keyart-v1.png to archive/art/pixel/; one archive/art/PROVENANCE.json (12 entries keyed relative to archive/art, original record plus sha256, bytes, archived_from, archived, reason). The keyart pin (visual_assets/tests.rs:363) retargets to the archive path. 12 legacy WAVs to archive/audio/ with archive/audio/PROVENANCE.json (12 records plus 8 preserved_originals). archive/README.md: "Not shipped; retained by production-pass decision 10; restore with git mv". archive/ is outside assets/, so packaging needs no change.
- **Supersedes in this spec:** archive/audio/retired/ subfolder.

#### S10. NpcShot retirement

- **Owner / wave:** audio, wave 1.
- **Ruling:** Separate audio commit (W1-58) right after the archive: runner/runtime.rs:390-392 drops the Npc =&gt; NpcShot arm so NPC shots use profile.weapon.class; Cue::NpcShot keeps its slot and gains Cue::retired(); retired cues are excluded from runtime paths and gate tables. survival_npc_shot_{0,1,2}.wav move to archive/audio/ with their records in a new archive/audio/SURVIVAL_PROVENANCE.json (reason "Cue::NpcShot retired; NPCs use their weapon class (user 2026-11-04)"). Shipped WAV count becomes 75.
- **Supersedes in this spec:** Wave-6 NpcShot move to archive/audio/retired/.

#### S11. Shipped audio provenance layout

- **Owner / wave:** audio, wave 6.
- **Ruling:** Two shipped manifests forever, one writer each: assets/audio/PROVENANCE.json (Python family: the 6 beds, written by the atmosphere script; LF-normalised generator_sha256) and assets/audio/SURVIVAL_PROVENANCE.json (Rust family: exporter). In wave 1 the survival file keeps its schema minus 3 npc_shot records (69) and documents its stale generator_sha256 in a note key. In wave 6 the exporter rewrites it as schema 2 per file, after copying v1 verbatim to archive/audio/SURVIVAL_PROVENANCE.v1.json.
- **Supersedes in this spec:** §10 single schema-2 PROVENANCE.json with SURVIVAL_PROVENANCE.json removed.

#### S12. assets/CREDITS.md lines

- **Owner / wave:** qa-captures, wave 1.
- **Ruling:** A generated block between &lt;!-- asset-index:begin --&gt; and &lt;!-- asset-index:end --&gt; with one line per shipped .wav/.png, rendered from the provenance manifests by the ignored writer test asset_audit::regenerate_credits_index; guard credits_index_matches_provenance. qa writes it in the archive commit and rewrites the stale prose plus an "Archived (not shipped)" paragraph; audio refreshes it in W1-58 and A6-05; vfx in any art commit.
- **Supersedes in this spec:** CREDITS.md:40-53,90-95 rewrite only in wave 7.

#### S13. Generator retargeting and --check

- **Owner / wave:** qa-captures, wave 1.
- **Ruling:** In the archive commit both Python generators get ASSETS and ARCHIVE roots and explicit owned-file lists; generate_audio.py renders ambience to assets and the 8 legacy clips to archive; generate_atmosphere_audio.py renders loops to assets and its 4 cues to archive and writes only the two Python manifests. --check is the default (render to a temp dir, compare sha256 and manifest fields, exit 1 on mismatch, write nothing); --write is explicit. Invoked as python -I -B; both --check runs join the full verify.
- **Supersedes in this spec:** Wave-1 step 3 (second owner of the same scripts).

#### S19. Audition reel and other non-listed files

- **Owner / wave:** audio, wave 6.
- **Ruling:** Only user-listed files are archived. docs/audio/survival-audition.{wav,csv} stay in place, labelled "v1 reel (pre-production-pass)"; new reels go only to target/audio-audition/ (uncommitted); wave-7 packaging excludes docs/audio/.
- **Supersedes in this spec:** §10 move of docs/audio/survival-audition.* to archive/audio/audition/.

### Cluster: enemies-world-pacing

#### W-13. Visual twins for EnemyWindup and BossPhase

- **Owner / wave:** vfx-lighting, wave 2.
- **Ruling:** The existing windup ellipse and bar already draws for every Windup enemy, defender-targeted included (critic claim incorrect). Added: EnemyView.aim_target dashed aim line for ranged windups (W2-28); BattleView.boss_phase: Option&lt;BossPhaseView{transition, name, hold_left_ticks}&gt; with label "PHASE n - NAME" and a ring for the whole 90-tick hold, static in reduced mode (W2-37). They precede the audio cues; the UX boss banner is not the twin.
- **Supersedes in this spec:** w5 dependency "visual twins owned by vfx" with no vfx step; the claim that only structure windups have the ring.

### Rulings owned by this remit

U2, U3, U4, R3, R9, P8, S5, S10, S11, S19
