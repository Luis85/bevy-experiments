//! Replays: record, encode, decode and verify; divergence, non-reproducible replays, header
//! checks and the JSON view (spec §5.5, §15 B, ADR-0007); format 4's versioned scale params,
//! semantic checks and re-bless (ADR-0021).

use std::path::PathBuf;
use std::sync::Arc;

use bevy_app::App;
use fl_content::ContentDb;
use fl_math::{Fx, FxVec2};
use fl_protocol::{DebugSetTerrainCmd, HeaderMismatch, MoveCmd, PlayerId, SimId, Tick};
use fl_sim::{
    CommandRegistry, MatchHeaderRes, MatchSetup, REPLAY_FORMAT_VERSION, Replay, ReplayError,
    ReplayEvent, ReplayRecorder, ReplayStart, RunConfig, ScaleScenario, SemanticCheck, SimRunner,
    TestDriver, VerifyOutcome, build_sim_app, first_divergence, load_content, rebless_replay,
    replay_app, replay_inputs, setup_scenario, verify_replay,
};
use fl_test_utils::{ScenarioBuilder, ascii_terrain_content, base_content};
use fl_view::{CommandDraft, SimQuery, SimReply, TickDriver};

fn core_mod() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/core_mod")
}

fn content() -> Arc<ContentDb> {
    load_content(&[], &[core_mod()]).expect("core_mod loads")
}

fn v(x: i32, y: i32) -> FxVec2 {
    FxVec2::new(Fx::from_int(x), Fx::from_int(y))
}

fn sim(seed: u64, hash_interval: u32) -> App {
    let setup = MatchSetup {
        seed,
        cheats_allowed: false,
    };
    let run = RunConfig {
        hash_interval: Some(hash_interval),
        ..RunConfig::default()
    };
    build_sim_app(&[], content(), setup, run).expect("the sim builds")
}

/// Run `runner` for `ticks` ticks from `driver`, recording into `recorder`; player 0 moves #1
/// at ticks 5 and 37 and player 1 moves #2 at tick 20 (when those ticks come up).
fn play(
    runner: &mut SimRunner,
    driver: &mut TestDriver,
    recorder: &mut ReplayRecorder,
    ticks: u32,
) {
    for _ in 0..ticks {
        let t = driver.next_tick().0;
        let order = match t {
            5 => Some((0, 1, v(500, 500))),
            20 => Some((1, 2, v(100, 900))),
            37 => Some((0, 1, v(800, 200))),
            _ => None,
        };
        if let Some((player, unit, target)) = order {
            let kind = runner
                .world()
                .resource::<CommandRegistry>()
                .encode(&MoveCmd { target });
            driver.issue(CommandDraft {
                player: PlayerId(player),
                units: vec![SimId(unit)],
                kind,
                queue: false,
            });
        }
        let input = driver.poll().expect("the test driver always ticks");
        let output = runner.step(input.clone());
        recorder.record(&input, &output);
    }
}

/// 60 ticks of the duel, hashed every `hash_interval` ticks, with three moves.
fn recorded_duel(hash_interval: u32) -> Replay {
    let mut app = sim(42, hash_interval);
    setup_scenario(app.world_mut(), "core_test.duel").expect("duel sets up");
    let header = app.world().resource::<MatchHeaderRes>().0.clone();
    let mut recorder = ReplayRecorder::new(
        header,
        ReplayStart::Scenario("core_test.duel".to_owned()),
        hash_interval,
    );
    let mut runner = SimRunner::new(app);
    play(&mut runner, &mut TestDriver::new(), &mut recorder, 60);
    recorder.finish()
}

fn verify(r: &Replay) -> Result<VerifyOutcome, ReplayError> {
    verify_replay(r, &[], content())
}

#[test]
fn replay_round_trip_verifies() {
    let replay = recorded_duel(1);
    assert_eq!(replay.version, REPLAY_FORMAT_VERSION);
    assert_eq!(replay.ticks, 60);
    assert_eq!(replay.hashes.len(), 60);
    let ticks: Vec<u32> = replay.inputs.iter().map(|i| i.tick.0).collect();
    assert_eq!(ticks, [5, 20, 37], "non-empty inputs only");
    assert!(replay.reproducible());

    let bytes = replay.to_bytes();
    let decoded = Replay::from_bytes(&bytes).expect("the replay decodes");
    assert_eq!(decoded, replay);
    assert_eq!(
        verify(&decoded).expect("the replay plays back"),
        VerifyOutcome::Verified {
            ticks: 60,
            warnings: Vec::new()
        }
    );
}

#[test]
fn replay_bytes_are_checked() {
    let bytes = recorded_duel(20).to_bytes();
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(matches!(
        Replay::from_bytes(&trailing),
        Err(ReplayError::Decode(_))
    ));
    assert!(matches!(
        Replay::from_bytes(&bytes[..bytes.len() / 2]),
        Err(ReplayError::Decode(_))
    ));
    // The version is the first field, a one-byte postcard varint while it is below 128.
    let next = REPLAY_FORMAT_VERSION + 1;
    let mut other_version = bytes;
    other_version[0] = u8::try_from(next).expect("a one-byte varint");
    assert!(matches!(
        Replay::from_bytes(&other_version),
        Err(ReplayError::Version { found }) if found == next
    ));
}

#[test]
fn tampered_replay_reports_divergence() {
    let mut replay = recorded_duel(1);
    let registry_sim = sim(42, 1);
    let kind = registry_sim
        .world()
        .resource::<CommandRegistry>()
        .encode(&MoveCmd {
            target: v(810, 200),
        });
    let input = replay
        .inputs
        .iter_mut()
        .find(|input| input.tick == Tick(37))
        .expect("an input at tick 37");
    input.commands[0].kind = kind;

    match verify(&replay).expect("the replay plays back") {
        VerifyOutcome::Diverged(divergence) => {
            assert_eq!(divergence.tick, Tick(37));
            assert_ne!(divergence.expected, divergence.actual);
        }
        other => panic!("expected a divergence, got {other:?}"),
    }
}

#[test]
fn replay_with_reload_event_is_non_reproducible() {
    let mut replay = recorded_duel(20);
    replay.events.push(ReplayEvent::ContentReload {
        tick: Tick(30),
        rules_hash: 1,
    });
    assert!(!replay.reproducible());
    assert_eq!(
        verify(&replay).expect("no error"),
        VerifyOutcome::NonReproducible
    );
}

#[test]
fn replay_build_id_mismatch_warns() {
    let mut replay = recorded_duel(20);
    replay.header.build_id = "another-build".to_owned();
    match verify(&replay).expect("the replay plays back") {
        VerifyOutcome::Verified { ticks, warnings } => {
            assert_eq!(ticks, 60);
            assert_eq!(warnings.len(), 1, "{warnings:?}");
        }
        other => panic!("expected a verified replay, got {other:?}"),
    }
}

#[test]
fn replay_rules_hash_mismatch_fails() {
    let mut replay = recorded_duel(20);
    replay.header.rules_hash ^= 1;
    assert!(matches!(
        verify(&replay),
        Err(ReplayError::Header(HeaderMismatch::RulesHash))
    ));
}

#[test]
fn replay_app_checks_header_and_sets_up() {
    let replay = recorded_duel(20);
    let (app, warnings) = replay_app(&replay, &[], content(), RunConfig::default())
        .expect("the replay's sim is set up");
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(!app.world().resource::<fl_sim::SimIndex>().is_empty());

    let mut other_build = replay.clone();
    other_build.header.build_id = "another-build".to_owned();
    let (_, warnings) = replay_app(&other_build, &[], content(), RunConfig::default())
        .expect("a build id mismatch only warns");
    assert_eq!(warnings.len(), 1, "{warnings:?}");

    let mut other_rules = replay;
    other_rules.header.rules_hash ^= 1;
    assert!(matches!(
        replay_app(&other_rules, &[], content(), RunConfig::default()),
        Err(ReplayError::Header(HeaderMismatch::RulesHash))
    ));
}

#[test]
fn replay_without_hashes_does_not_verify() {
    let mut replay = recorded_duel(20);
    replay.hashes.clear();
    assert!(matches!(verify(&replay), Err(ReplayError::Setup(_))));
}

#[test]
fn first_divergence_finds_earliest() {
    let a = [(Tick(3), 4), (Tick(0), 1), (Tick(1), 2), (Tick(2), 3)];
    let b = [(Tick(0), 1), (Tick(1), 2), (Tick(2), 9), (Tick(3), 8)];
    let divergence = first_divergence(&a, &b).expect("they diverge");
    assert_eq!(divergence.tick, Tick(2));
    assert_eq!(divergence.expected, 3);
    assert_eq!(divergence.actual, 9);
    assert_eq!(first_divergence(&a, &a), None);
    assert_eq!(
        first_divergence(&a, &[(Tick(1), 2)]),
        None,
        "ticks only one side hashed are not compared"
    );
}

#[test]
fn replay_json_lists_inputs() {
    let json = recorded_duel(20).to_json();
    assert!(
        json["inputs"][0]["commands"][0]["seq"].is_number(),
        "{json}"
    );
    assert_eq!(json["inputs"][0]["tick"], 5);
    assert_eq!(json["start"]["Scenario"], "core_test.duel");
    assert_eq!(json["ticks"], 60);
}

#[test]
fn scale_and_save_starts_verify() {
    // A scale scenario.
    let scale = ScaleScenario {
        units: 40,
        players: 2,
        map_m: 400,
        wander_m: 30,
        seed: 9,
        followers_per_unit: 2,
        projectiles: 12,
        projectile_ttl: 4,
        sensor_every: 5,
        map: None,
    };
    let mut app = sim(9, 5);
    scale
        .populate(app.world_mut())
        .expect("the scale scenario sets up");
    let header = app.world().resource::<MatchHeaderRes>().0.clone();
    let mut recorder = ReplayRecorder::new(header, ReplayStart::scale(&scale), 5);
    let mut runner = SimRunner::new(app);
    let mut driver = TestDriver::new();
    play(&mut runner, &mut driver, &mut recorder, 30);
    assert!(matches!(
        verify(&recorder.finish()),
        Ok(VerifyOutcome::Verified { ticks: 30, .. })
    ));

    // A save made at tick 30 of that match, played on (with a move at tick 37).
    let pending = driver.pending();
    let SimReply::Save(save) = runner.query(SimQuery::Save { pending }) else {
        panic!("the save answers");
    };
    let header = runner.world().resource::<MatchHeaderRes>().0.clone();
    let mut recorder = ReplayRecorder::new(header, ReplayStart::Save(save), 5);
    play(&mut runner, &mut driver, &mut recorder, 30);
    let replay = recorder.finish();
    assert_eq!(replay.hashes.first().map(|h| h.0), Some(Tick(30)));
    assert!(replay.inputs.iter().any(|i| i.tick == Tick(37)));
    assert!(matches!(
        verify(&replay),
        Ok(VerifyOutcome::Verified { ticks: 30, .. })
    ));
}

/// `"aaa.noop"`: does nothing; its name sorts before every core command.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct NoopCmd;

impl fl_protocol::CommandPayload for NoopCmd {
    const NAME: &'static str = "aaa.noop";
    const NEEDS_UNITS: bool = false;
}

struct NoopHandler;

impl fl_sim::CommandHandler<NoopCmd> for NoopHandler {
    fn apply(
        &self,
        _payload: &NoopCmd,
        _world: &mut bevy_ecs::world::World,
        _ctx: &fl_sim::CommandCtx<'_>,
    ) -> Result<(), fl_sim::RejectReason> {
        Ok(())
    }
}

/// Feature `"aaa"` (version 1), which registers `"aaa.noop"` only when `.0` is set: the same
/// feature list with a different command table.
struct NoopFeature(bool);

impl fl_sim::SimFeature for NoopFeature {
    fn id(&self) -> &'static str {
        "aaa"
    }

    fn version(&self) -> u32 {
        1
    }

    fn build(&self, sim: &mut fl_sim::SimBuilder) {
        if self.0 {
            sim.register_command::<NoopCmd>(NoopHandler);
        }
    }
}

fn noop_features(with_noop: bool) -> Vec<Arc<dyn fl_sim::SimFeature>> {
    vec![Arc::new(NoopFeature(with_noop))]
}

/// [`recorded_duel`] (hash every tick) with `NoopFeature(false)`.
fn recorded_noop_duel() -> Replay {
    let setup = MatchSetup {
        seed: 42,
        cheats_allowed: false,
    };
    let run = RunConfig {
        hash_interval: Some(1),
        ..RunConfig::default()
    };
    let features = noop_features(false);
    let mut app = build_sim_app(&features, content(), setup, run).expect("the sim builds");
    setup_scenario(app.world_mut(), "core_test.duel").expect("duel sets up");
    let header = app.world().resource::<MatchHeaderRes>().0.clone();
    let mut recorder = ReplayRecorder::new(
        header,
        ReplayStart::Scenario("core_test.duel".to_owned()),
        1,
    );
    let mut runner = SimRunner::new(app);
    play(&mut runner, &mut TestDriver::new(), &mut recorder, 60);
    recorder.finish()
}

#[test]
fn replay_remaps_command_ids_by_name() {
    let replay = recorded_noop_duel();
    let features = noop_features(true);
    let (app, _) = replay_app(&replay, &features, content(), RunConfig::default())
        .expect("the replay sets up");
    let inputs = replay_inputs(&replay, &app).expect("every recorded command maps");
    let move_id = app
        .world()
        .resource::<CommandRegistry>()
        .type_id::<MoveCmd>();
    let recorded_id = replay.inputs[0].commands[0].kind.type_id;
    assert_ne!(
        recorded_id, move_id,
        "aaa.noop sorts first and shifts core.move"
    );
    assert!(
        inputs
            .iter()
            .flat_map(|input| &input.commands)
            .all(|cmd| cmd.kind.type_id == move_id)
    );
    assert_eq!(
        verify_replay(&replay, &features, content()).expect("the replay plays back"),
        VerifyOutcome::Verified {
            ticks: 60,
            warnings: Vec::new()
        }
    );
}

#[test]
fn replay_with_unknown_command_name_is_an_error() {
    let mut replay = recorded_noop_duel();
    let commands = replay
        .header
        .string_tables
        .tables
        .get_mut(fl_protocol::tables::COMMANDS)
        .expect("the commands table");
    let name = commands
        .iter_mut()
        .find(|name| *name == "core.move")
        .expect("core.move");
    "core.gone".clone_into(name);
    match verify_replay(&replay, &noop_features(false), content()) {
        Err(ReplayError::Command(message)) => assert!(message.contains("core.gone"), "{message}"),
        other => panic!("expected ReplayError::Command, got {other:?}"),
    }
}

/// The testbed wander golden as committed at tag `m1`: replay format 3.
fn format_3_fixture() -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/replays/testbed_wander_format3.replay");
    std::fs::read(&path).unwrap_or_else(|err| panic!("reading {}: {err}", path.display()))
}

/// The sha256 of the format-3 fixture: `m1:crates/fl_test_utils/tests/replays/testbed_wander.replay`.
const FORMAT_3_SHA256: &str = "921e082afa22877131f8b43a7b94f4d411af48b41d0759a52091131f6cc89e61";

/// `bytes` as lowercase hex.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// SHA-256 (FIPS 180-4) of `data`; the workspace has no hashing crate for it, and the fixture
/// is pinned by the digest `sha256sum` prints.
fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a_2f98,
        0x7137_4491,
        0xb5c0_fbcf,
        0xe9b5_dba5,
        0x3956_c25b,
        0x59f1_11f1,
        0x923f_82a4,
        0xab1c_5ed5,
        0xd807_aa98,
        0x1283_5b01,
        0x2431_85be,
        0x550c_7dc3,
        0x72be_5d74,
        0x80de_b1fe,
        0x9bdc_06a7,
        0xc19b_f174,
        0xe49b_69c1,
        0xefbe_4786,
        0x0fc1_9dc6,
        0x240c_a1cc,
        0x2de9_2c6f,
        0x4a74_84aa,
        0x5cb0_a9dc,
        0x76f9_88da,
        0x983e_5152,
        0xa831_c66d,
        0xb003_27c8,
        0xbf59_7fc7,
        0xc6e0_0bf3,
        0xd5a7_9147,
        0x06ca_6351,
        0x1429_2967,
        0x27b7_0a85,
        0x2e1b_2138,
        0x4d2c_6dfc,
        0x5338_0d13,
        0x650a_7354,
        0x766a_0abb,
        0x81c2_c92e,
        0x9272_2c85,
        0xa2bf_e8a1,
        0xa81a_664b,
        0xc24b_8b70,
        0xc76c_51a3,
        0xd192_e819,
        0xd699_0624,
        0xf40e_3585,
        0x106a_a070,
        0x19a4_c116,
        0x1e37_6c08,
        0x2748_774c,
        0x34b0_bcb5,
        0x391c_0cb3,
        0x4ed8_aa4a,
        0x5b9c_ca4f,
        0x682e_6ff3,
        0x748f_82ee,
        0x78a5_636f,
        0x84c8_7814,
        0x8cc7_0208,
        0x90be_fffa,
        0xa450_6ceb,
        0xbef9_a3f7,
        0xc671_78f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09_e667,
        0xbb67_ae85,
        0x3c6e_f372,
        0xa54f_f53a,
        0x510e_527f,
        0x9b05_688c,
        0x1f83_d9ab,
        0x5be0_cd19,
    ];
    let mut message = data.to_vec();
    let bits = u64::try_from(data.len())
        .expect("a fixture under 2^64 bytes")
        .wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bits.to_be_bytes());
    for block in message.as_chunks::<64>().0 {
        let mut w = [0_u32; 64];
        for (slot, word) in w.iter_mut().zip(block.as_chunks::<4>().0) {
            *slot = u32::from_be_bytes(*word);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for (&k, &wi) in K.iter().zip(&w) {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(k)
                .wrapping_add(wi);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (state, add) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *state = state.wrapping_add(add);
        }
    }
    let mut out = [0_u8; 32];
    for (chunk, word) in out.as_chunks_mut::<4>().0.iter_mut().zip(h) {
        *chunk = word.to_be_bytes();
    }
    out
}

#[test]
fn sha256_matches_known_digests() {
    assert_eq!(
        hex(&sha256(b"")),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        hex(&sha256(b"abc")),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

/// The hash after tick 0 stored in the format-3 fixture (the base testbed's: the committed
/// golden verifies against this build with it).
const FORMAT_3_FIRST_HASH: u64 = 0x57ef_0ca1_ac59_fc73;
/// The hash after tick 399 stored in the format-3 fixture.
const FORMAT_3_LAST_HASH: u64 = 0xfe27_3928_e11a_01d1;

#[test]
fn format_3_replays_still_read() {
    let bytes = format_3_fixture();
    assert_eq!(bytes.len(), 6552);
    assert_eq!(
        hex(&sha256(&bytes)),
        FORMAT_3_SHA256,
        "the fixture is the tag-m1 golden, byte for byte"
    );
    assert_eq!(bytes[0], 3, "the fixture is format 3");
    let replay = Replay::from_bytes(&bytes).expect("format 3 still reads");
    assert_eq!(
        replay.version, REPLAY_FORMAT_VERSION,
        "read as the current format"
    );
    assert!(replay.semantic.is_empty());
    assert!(replay.journal_summary.is_empty());
    assert_eq!(
        replay.start,
        ReplayStart::Scenario("base.testbed".to_owned())
    );
    assert_eq!(replay.ticks, 400);
    assert_eq!(replay.hashes.len(), 400);
    assert_eq!(replay.hashes[0], (Tick(0), FORMAT_3_FIRST_HASH));
    assert_eq!(replay.hashes[399], (Tick(399), FORMAT_3_LAST_HASH));
    let ticks: Vec<Tick> = replay.inputs.iter().map(|input| input.tick).collect();
    assert_eq!(ticks, [Tick(0), Tick(10), Tick(60)]);
    // Written back, it is a format-4 replay with the same content.
    let upgraded = Replay::from_bytes(&replay.to_bytes()).expect("format 4 reads");
    assert_eq!(upgraded, replay);
}

#[test]
fn scale_start_round_trips_versioned_params() {
    let scale = ScaleScenario::with_units(50);
    let (version, params) = scale.encode();
    assert_eq!(version, ScaleScenario::VERSION);
    assert_eq!(version, 1);
    // Version 1's bytes, pinned: units 50, players 4, map_m 2000 and wander_m 150 (zigzag
    // varints), seed 1, followers 2, projectiles 16, ttl 20, sensor_every 10, map None.
    assert_eq!(
        params,
        [
            0x32, 0x04, 0xa0, 0x1f, 0xac, 0x02, 0x01, 0x02, 0x10, 0x14, 0x0a, 0x00
        ]
    );
    assert_eq!(ScaleScenario::decode(version, &params), Ok(scale.clone()));
    assert_eq!(
        ReplayStart::scale(&scale),
        ReplayStart::Scale { version: 1, params }
    );
}

#[test]
fn scale_params_of_unknown_version_fail() {
    let (_, params) = ScaleScenario::with_units(50).encode();
    let err = ScaleScenario::decode(2, &params).expect_err("version 2 is unknown");
    assert!(err.contains("ScaleScenario version 2"), "{err}");
    let mut trailing = params;
    trailing.push(0);
    assert!(
        ScaleScenario::decode(1, &trailing).is_err(),
        "trailing bytes"
    );
}

fn unit_exists(id: &str, tick: u32, unit: u32, exists: bool) -> SemanticCheck {
    SemanticCheck::UnitExists {
        id: id.to_owned(),
        tick: Tick(tick),
        unit: SimId(unit),
        exists,
    }
}

#[test]
fn semantic_failure_is_reported() {
    let passing = vec![
        unit_exists("one_alive", 5, 1, true),
        SemanticCheck::EntityCount {
            id: "duel".to_owned(),
            tick: Tick(59),
            min: 2,
            max: 2,
        },
        SemanticCheck::UnitWithin {
            id: "one_on_map".to_owned(),
            tick: Tick(30),
            unit: SimId(1),
            min: v(0, 0),
            max: v(1000, 1000),
        },
    ];
    let replay = recorded_duel(1).with_semantics(passing.clone());
    assert_eq!(replay.semantic, passing);
    assert!(
        matches!(
            verify(&replay),
            Ok(VerifyOutcome::Verified { ticks: 60, .. })
        ),
        "the true checks pass"
    );

    let replay = recorded_duel(1).with_semantics(vec![
        unit_exists("ghost", 5, 99, true),
        SemanticCheck::UnitWithin {
            id: "one_in_a_box".to_owned(),
            tick: Tick(30),
            unit: SimId(1),
            min: v(0, 0),
            max: v(1, 1),
        },
        SemanticCheck::EntityCount {
            id: "past_the_end".to_owned(),
            tick: Tick(60),
            min: 0,
            max: 9,
        },
    ]);
    let decoded = Replay::from_bytes(&replay.to_bytes()).expect("the checks encode");
    assert_eq!(decoded, replay);
    match verify(&decoded) {
        Ok(VerifyOutcome::SemanticFailed(failures)) => {
            let ids: Vec<(&str, Tick)> = failures.iter().map(|f| (f.id.as_str(), f.tick)).collect();
            assert_eq!(
                ids,
                [
                    ("ghost", Tick(5)),
                    ("one_in_a_box", Tick(30)),
                    ("past_the_end", Tick(60))
                ]
            );
            assert!(
                failures.iter().all(|f| !f.detail.is_empty()),
                "{failures:?}"
            );
        }
        other => panic!("expected a semantic failure, got {other:?}"),
    }
}

#[test]
fn semantic_checks_run_after_the_hashes() {
    let mut replay = recorded_duel(1).with_semantics(vec![unit_exists("ghost", 5, 99, true)]);
    replay.hashes[10].1 ^= 1;
    assert!(
        matches!(verify(&replay), Ok(VerifyOutcome::Diverged(d)) if d.tick == Tick(10)),
        "a divergence is reported before a failed check"
    );
}

#[test]
fn never_on_terrain_checks_every_tick() {
    let rows = ["....", "...."];
    // At tick 3 the tank's cell (0, 0) becomes water under it: from then on it stands on
    // water (a teleport onto water is rejected, so the water comes to the tank).
    let mut s = ScenarioBuilder::new()
        .ascii_map(&rows, 4)
        .player("t.red", 0)
        .unit("t.tank", 0, 2, 2)
        .command(
            3,
            0,
            &[],
            DebugSetTerrainCmd {
                min_cell: (0, 0),
                max_cell: (0, 0),
                terrain: "t.water".to_owned(),
            },
        )
        .build();
    s.run_ticks(6);
    let dry = |terrain: &str| SemanticCheck::NeverOnTerrain {
        id: "dry".to_owned(),
        units: vec![SimId(1), SimId(7)],
        terrain: terrain.to_owned(),
    };
    let content = ascii_terrain_content(&rows, 4);
    let replay = s.replay().with_semantics(vec![dry("t.swamp")]);
    assert!(matches!(
        verify_replay(&replay, s.features(), content.clone()),
        Ok(VerifyOutcome::Verified { ticks: 6, .. })
    ));
    // A terrain name the content does not have could never match: the check fails at once.
    let replay = s.replay().with_semantics(vec![dry("t.lava")]);
    match verify_replay(&replay, s.features(), content.clone()) {
        Ok(VerifyOutcome::SemanticFailed(failures)) => {
            assert_eq!(failures.len(), 1, "{failures:?}");
            assert_eq!(failures[0].tick, Tick(0));
            assert!(failures[0].detail.contains("t.lava"), "{failures:?}");
        }
        other => panic!("expected a semantic failure, got {other:?}"),
    }
    let replay = s.replay().with_semantics(vec![dry("t.water")]);
    match verify_replay(&replay, s.features(), content) {
        Ok(VerifyOutcome::SemanticFailed(failures)) => {
            assert_eq!(failures.len(), 1, "one failure per check: {failures:?}");
            assert_eq!(failures[0].id, "dry");
            assert_eq!(failures[0].tick, Tick(3));
            assert!(failures[0].detail.contains("t.water"), "{failures:?}");
        }
        other => panic!("expected a semantic failure, got {other:?}"),
    }
}

#[test]
fn rebless_keeps_hashes_for_unchanged_code() {
    let mut s = ScenarioBuilder::new()
        .scenario("base.testbed")
        .wander_all(100)
        .command(
            10,
            0,
            &[1, 2],
            MoveCmd {
                target: v(600, 500),
            },
        )
        .build();
    s.run_ticks(60);
    let checks = vec![SemanticCheck::EntityCount {
        id: "ten_units".to_owned(),
        tick: Tick(59),
        min: 10,
        max: 10,
    }];
    let replay = s.replay().with_semantics(checks.clone());
    let (reblessed, entry) =
        rebless_replay(&replay, s.features(), base_content()).expect("the replay re-runs");
    assert_eq!(entry.first_divergence, None);
    assert!(entry.semantic.is_empty(), "{:?}", entry.semantic);
    assert_eq!(entry.entities, (10, 10));
    assert_eq!(reblessed.hashes, replay.hashes);
    assert_eq!(reblessed.start, replay.start);
    assert_eq!(reblessed.inputs, replay.inputs);
    assert_eq!(reblessed.ticks, replay.ticks);
    assert_eq!(reblessed.semantic, checks);
    let orders = reblessed.journal_summary.get("core.order_changed");
    assert!(
        orders.is_some_and(|&n| n > 0),
        "the journal is on: {:?}",
        reblessed.journal_summary
    );
    assert!(matches!(
        verify_replay(&reblessed, s.features(), base_content()),
        Ok(VerifyOutcome::Verified { ticks: 60, .. })
    ));
}
