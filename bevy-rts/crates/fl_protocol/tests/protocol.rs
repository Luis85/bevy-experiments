//! Contract tests for the lockstep envelope, payloads, `MatchHeader` and string tables.

use fl_math::{Angle, Fx, FxVec2};
use fl_protocol::{
    CommandKind, CommandPayload, CommandTypeId, DebugInvulnerableCmd, DebugSetHpCmd,
    DebugSetOwnerCmd, DebugSetTerrainCmd, DebugSpawnCmd, DebugTeleportCmd, DebugWanderCmd,
    FORMAT_VERSION, FollowCmd, HeaderMismatch, MatchHeader, MoveCmd, PlayerCommand, PlayerId,
    RotateCmd, SimId, StopCmd, StringTables, Tick, tables,
};

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}

fn header() -> MatchHeader {
    let mut string_tables = StringTables::default();
    string_tables.insert(tables::COMMANDS, names(&["core.move", "core.stop"]));
    string_tables.insert(tables::UNITS, names(&["base.de.pz3", "base.su.t34"]));
    MatchHeader {
        format_version: FORMAT_VERSION,
        build_id: "build-a".to_owned(),
        sim_version: 1,
        features: vec![("core".to_owned(), 1), ("movement".to_owned(), 2)],
        mods: vec![("base".to_owned(), "0.1.0".to_owned())],
        rules_hash: 0xDEAD_BEEF,
        cheats_allowed: false,
        seed: 42,
        string_tables,
    }
}

#[test]
fn player_command_postcard_roundtrip() {
    let cmd = PlayerCommand {
        exec_tick: Tick(7),
        player: PlayerId(1),
        seq: 3,
        units: vec![SimId(3), SimId(9)],
        kind: CommandKind {
            type_id: CommandTypeId(2),
            payload: vec![1, 2, 3],
        },
        queue: true,
    };
    let bytes = postcard::to_allocvec(&cmd).expect("encodes");
    let back: PlayerCommand = postcard::from_bytes(&bytes).expect("decodes");
    assert_eq!(back, cmd);
    assert_eq!(back.exec_tick, Tick(7));
    assert_eq!(back.seq, 3);
}

#[test]
fn match_header_postcard_roundtrip() {
    let h = header();
    assert_eq!(h.features.len(), 2);
    assert_eq!(h.mods.len(), 1);
    assert_eq!(h.string_tables.tables.len(), 2);
    let bytes = postcard::to_allocvec(&h).expect("encodes");
    let back: MatchHeader = postcard::from_bytes(&bytes).expect("decodes");
    assert_eq!(back, h);
}

#[test]
fn payload_names() {
    assert_eq!(MoveCmd::NAME, "core.move");
    assert_eq!(StopCmd::NAME, "core.stop");
    assert_eq!(DebugSpawnCmd::NAME, "debug.spawn");
    assert_eq!(DebugWanderCmd::NAME, "debug.wander");
    assert_eq!(DebugSetOwnerCmd::NAME, "debug.set_owner");
    assert_eq!(DebugInvulnerableCmd::NAME, "debug.invulnerable");
    assert_eq!(DebugSetHpCmd::NAME, "debug.set_hp");
    assert_eq!(DebugTeleportCmd::NAME, "debug.teleport");
    assert_eq!(RotateCmd::NAME, "core.rotate");
    assert_eq!(FollowCmd::NAME, "core.follow");
    assert_eq!(DebugSetTerrainCmd::NAME, "debug.set_terrain");
}

#[test]
fn debug_payloads_are_debug_only() {
    // `assert_eq!` on arrays: clippy rejects `assert!` on a constant.
    let core = [MoveCmd::DEBUG_ONLY, StopCmd::DEBUG_ONLY];
    assert_eq!(core, [false; 2]);
    let debug = [
        DebugSpawnCmd::DEBUG_ONLY,
        DebugWanderCmd::DEBUG_ONLY,
        DebugSetOwnerCmd::DEBUG_ONLY,
        DebugInvulnerableCmd::DEBUG_ONLY,
        DebugSetHpCmd::DEBUG_ONLY,
        DebugTeleportCmd::DEBUG_ONLY,
    ];
    assert_eq!(debug, [true; 6]);

    let needs_units = [
        DebugSpawnCmd::NEEDS_UNITS,
        MoveCmd::NEEDS_UNITS,
        StopCmd::NEEDS_UNITS,
    ];
    assert_eq!(needs_units, [false, true, true]);
}

#[test]
fn debug_set_terrain_is_debug_only_without_units() {
    // `assert_eq!` on arrays: clippy rejects `assert!` on a constant.
    assert_eq!(
        [
            DebugSetTerrainCmd::DEBUG_ONLY,
            DebugSetTerrainCmd::NEEDS_UNITS
        ],
        [true, false]
    );
    assert_eq!([RotateCmd::DEBUG_ONLY, FollowCmd::DEBUG_ONLY], [false; 2]);
    assert_eq!([RotateCmd::NEEDS_UNITS, FollowCmd::NEEDS_UNITS], [true; 2]);
}

fn postcard_round_trip<T>(value: &T) -> T
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    let bytes = postcard::to_allocvec(value).expect("encodes");
    postcard::from_bytes(&bytes).expect("decodes")
}

#[test]
fn new_payloads_round_trip_postcard() {
    let rotate = RotateCmd {
        toward: FxVec2::new(Fx::from_raw(-7), Fx::from_int(3)),
    };
    let back = postcard_round_trip(&rotate);
    assert_eq!(back, rotate);
    assert_eq!((back.toward.x.raw(), back.toward.y), (-7, Fx::from_int(3)));

    let follow = FollowCmd { leader: SimId(9) };
    assert_eq!(postcard_round_trip(&follow), follow);

    let terrain = DebugSetTerrainCmd {
        min_cell: (1, 2),
        max_cell: (3, 4),
        terrain: "base.water".to_owned(),
    };
    assert_eq!(postcard_round_trip(&terrain), terrain);
}

#[test]
fn fx_payload_is_lossless_in_postcard() {
    let cmd = MoveCmd {
        target: FxVec2::new(Fx::from_raw(0x7FFF_FFFF_0000_0001), Fx::from_raw(-3)),
    };
    let bytes = postcard::to_allocvec(&cmd).expect("encodes");
    let back: MoveCmd = postcard::from_bytes(&bytes).expect("decodes");
    assert_eq!(back.target.x.raw(), 0x7FFF_FFFF_0000_0001);
    assert_eq!(back.target.y.raw(), -3);

    let spawn = DebugSpawnCmd {
        unit: "base.de.pz3".to_owned(),
        owner: PlayerId(2),
        pos: FxVec2::new(Fx::from_raw(1), Fx::from_raw(-1)),
        heading: Angle::default(),
    };
    let bytes = postcard::to_allocvec(&spawn).expect("encodes");
    let back: DebugSpawnCmd = postcard::from_bytes(&bytes).expect("decodes");
    assert_eq!(back, spawn);
}

#[test]
fn payload_rejects_unknown_fields_in_ron() {
    let ok: MoveCmd = ron::from_str("(target: (1.0, 2.0))").expect("known fields parse");
    assert_eq!(ok.target, FxVec2::new(Fx::from_int(1), Fx::from_int(2)));
    assert!(ron::from_str::<MoveCmd>("(target: (1.0, 2.0), extra: 1)").is_err());
}

#[test]
fn string_tables_id_of_and_remap() {
    let mut old = StringTables::default();
    old.insert(tables::UNITS, names(&["a", "c"]));
    let mut current = StringTables::default();
    current.insert(tables::UNITS, names(&["a", "b", "c"]));

    assert_eq!(current.id_of(tables::UNITS, "b"), Some(1));
    assert_eq!(current.id_of(tables::UNITS, "x"), None);
    assert_eq!(current.id_of(tables::COMMANDS, "a"), None);
    assert_eq!(
        current.get(tables::UNITS),
        Some(names(&["a", "b", "c"]).as_slice())
    );

    assert_eq!(old.remap(tables::UNITS, &current), [Some(0), Some(2)]);
    assert_eq!(current.remap(tables::UNITS, &old), [Some(0), None, Some(1)]);
}

#[test]
fn replay_check_warns_on_build_id_only() {
    let ours = header();
    let mut recorded = header();
    recorded.build_id = "build-b".to_owned();
    // Fields a replay takes from its own header do not matter.
    recorded.seed = 7;
    recorded.cheats_allowed = true;
    let warnings = ours.check_replay(&recorded).expect("compatible");
    assert_eq!(warnings.len(), 1);
    assert_eq!(ours.check_replay(&header()), Ok(Vec::new()));
}

#[test]
fn replay_check_rejects_rules_hash_mismatch() {
    let ours = header();
    let mut recorded = header();
    recorded.rules_hash ^= 1;
    assert_eq!(ours.check_replay(&recorded), Err(HeaderMismatch::RulesHash));
}

#[test]
fn handshake_requires_build_id() {
    let ours = header();
    let mut peer = header();
    peer.build_id = "build-b".to_owned();
    assert_eq!(ours.check_handshake(&peer), Err(HeaderMismatch::BuildId));
    assert_eq!(ours.check_handshake(&header()), Ok(()));
}

#[test]
fn save_check_rejects_feature_mismatch() {
    let ours = header();
    let mut saved = header();
    saved.features[1].1 = 3;
    assert_eq!(ours.check_save(&saved), Err(HeaderMismatch::Features));

    let mut older = header();
    older.format_version = 0;
    older.build_id = "build-b".to_owned();
    assert_eq!(ours.check_save(&older), Ok(()));
    let mut newer = header();
    newer.format_version = FORMAT_VERSION + 1;
    assert_eq!(ours.check_save(&newer), Err(HeaderMismatch::FormatVersion));
}

#[test]
fn save_check_accepts_older_feature_versions() {
    // R70: a save made by an older version of a feature loads; its codecs' versions and
    // migrations decide what decodes.
    let ours = header();
    let mut older = header();
    older.features[1].1 = 1;
    assert_eq!(ours.check_save(&older), Ok(()));

    // The feature ids must still be the same.
    let mut renamed = header();
    renamed.features[1].0 = "movement2".to_owned();
    assert_eq!(ours.check_save(&renamed), Err(HeaderMismatch::Features));
    let mut extra = header();
    extra.features.push(("artillery".to_owned(), 1));
    assert_eq!(ours.check_save(&extra), Err(HeaderMismatch::Features));
    let mut missing = header();
    missing.features.pop();
    assert_eq!(ours.check_save(&missing), Err(HeaderMismatch::Features));
}

#[test]
fn replay_and_handshake_stay_strict_on_feature_versions() {
    let ours = header();
    let mut older = header();
    older.features[1].1 = 1;
    assert_eq!(ours.check_replay(&older), Err(HeaderMismatch::Features));
    assert_eq!(ours.check_handshake(&older), Err(HeaderMismatch::Features));
}
