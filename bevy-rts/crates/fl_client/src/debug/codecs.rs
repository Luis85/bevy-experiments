//! JSON command codecs: a command payload given by name and as JSON (`fl.issue_command`)
//! becomes a postcard [`CommandKind`] issued through [`LocalDriver`] (spec §15 C, ADR-0008).
use std::collections::BTreeMap;

use bevy::prelude::*;
use fl_protocol::{
    CommandPayload, DebugInvulnerableCmd, DebugSetHpCmd, DebugSetOwnerCmd, DebugSetTerrainCmd,
    DebugSpawnCmd, DebugTeleportCmd, DebugWanderCmd, FollowCmd, MatchHeader, MoveCmd,
    PlayerCommand, PlayerId, RotateCmd, SimId, StopCmd,
};
use fl_view::{CommandDraft, TickDriver};

use crate::driver::LocalDriver;
use crate::link::{RunnerLink, encode_command_bytes};

/// Decodes a payload from JSON and encodes it as postcard.
pub type CommandJsonFn = fn(serde_json::Value) -> Result<Vec<u8>, String>;

/// How one command kind is encoded from JSON.
#[derive(Clone, Copy, Debug)]
pub struct CommandCodec {
    pub encode: CommandJsonFn,
    /// [`CommandPayload::DEBUG_ONLY`]: accepted only when the match allows cheats.
    pub debug_only: bool,
}

/// The JSON codecs of the command kinds the client can issue by name, by command name.
#[derive(Resource, Default)]
pub struct ClientCommandCodecs {
    codecs: BTreeMap<&'static str, CommandCodec>,
}

impl ClientCommandCodecs {
    /// Adds (or replaces) the codec of `P`.
    pub fn insert<P: CommandPayload>(&mut self) {
        self.codecs.insert(
            P::NAME,
            CommandCodec {
                encode: json_to_postcard::<P>,
                debug_only: P::DEBUG_ONLY,
            },
        );
    }

    /// The codec of the command `name`.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<CommandCodec> {
        self.codecs.get(name).copied()
    }

    /// Every command name with a codec, in name order.
    pub fn names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.codecs.keys().copied()
    }

    /// The [`CommandKind`](fl_protocol::CommandKind) of the command `name` with the JSON
    /// `payload`, for the match of `header`.
    ///
    /// # Errors
    ///
    /// If no codec has that name, the command is `debug.*` and the match disallows cheats,
    /// the payload does not decode, or the match's string table lacks the command.
    pub fn encode(
        &self,
        header: &MatchHeader,
        name: &str,
        payload: serde_json::Value,
    ) -> Result<fl_protocol::CommandKind, String> {
        let codec = self.get(name).ok_or_else(|| {
            let known: Vec<&str> = self.names().collect();
            format!(
                "unknown command '{name}'; known commands: {}",
                known.join(", ")
            )
        })?;
        if codec.debug_only && !header.cheats_allowed {
            return Err(format!("'{name}' needs cheats; this match disallows them"));
        }
        let bytes =
            (codec.encode)(payload).map_err(|err| format!("command '{name}' payload: {err}"))?;
        encode_command_bytes(&header.string_tables, name, bytes)
    }
}

fn json_to_postcard<P: CommandPayload>(value: serde_json::Value) -> Result<Vec<u8>, String> {
    let payload: P = serde_json::from_value(value).map_err(|err| err.to_string())?;
    postcard::to_allocvec(&payload).map_err(|err| err.to_string())
}

/// Registers JSON command codecs on an [`App`].
pub trait CommandCodecAppExt {
    /// Makes the command `P` issuable by name with a JSON payload.
    fn register_command_json<P: CommandPayload>(&mut self) -> &mut Self;
}

impl CommandCodecAppExt for App {
    fn register_command_json<P: CommandPayload>(&mut self) -> &mut Self {
        self.world_mut()
            .get_resource_or_init::<ClientCommandCodecs>()
            .insert::<P>();
        self
    }
}

/// Registers the codecs of the core and `debug.*` payloads.
pub(crate) fn register_builtin_codecs(app: &mut App) {
    app.register_command_json::<MoveCmd>()
        .register_command_json::<StopCmd>()
        .register_command_json::<RotateCmd>()
        .register_command_json::<FollowCmd>()
        .register_command_json::<DebugSetTerrainCmd>()
        .register_command_json::<DebugSpawnCmd>()
        .register_command_json::<DebugWanderCmd>()
        .register_command_json::<DebugSetOwnerCmd>()
        .register_command_json::<DebugInvulnerableCmd>()
        .register_command_json::<DebugSetHpCmd>()
        .register_command_json::<DebugTeleportCmd>();
}

/// Stamps the command `name` with the JSON `payload` as a command of `player` for `units` and
/// queues it in the [`LocalDriver`] of `world`.
///
/// # Errors
///
/// If no match is running, or [`ClientCommandCodecs::encode`] fails.
pub fn issue_json(
    world: &mut World,
    player: PlayerId,
    units: Vec<SimId>,
    name: &str,
    payload: serde_json::Value,
) -> Result<PlayerCommand, String> {
    let (Some(link), Some(codecs)) = (
        world.get_resource::<RunnerLink>(),
        world.get_resource::<ClientCommandCodecs>(),
    ) else {
        return Err("no match is running".to_owned());
    };
    let kind = codecs.encode(&link.header, name, payload)?;
    let mut driver = world
        .get_resource_mut::<LocalDriver>()
        .ok_or("no match is running")?;
    Ok(driver.issue(CommandDraft {
        player,
        units,
        kind,
        queue: false,
    }))
}

#[cfg(test)]
mod tests {
    use fl_protocol::{StringTables, tables};
    use serde_json::json;

    use super::*;

    fn header(cheats_allowed: bool) -> MatchHeader {
        let mut string_tables = StringTables::default();
        string_tables.insert(
            tables::COMMANDS,
            vec!["core.move".to_owned(), "debug.set_hp".to_owned()],
        );
        MatchHeader {
            format_version: fl_protocol::FORMAT_VERSION,
            build_id: "test".to_owned(),
            sim_version: 1,
            features: Vec::new(),
            mods: Vec::new(),
            rules_hash: 0,
            cheats_allowed,
            seed: 1,
            string_tables,
        }
    }

    fn codecs() -> ClientCommandCodecs {
        let mut app = App::new();
        register_builtin_codecs(&mut app);
        app.world_mut()
            .remove_resource::<ClientCommandCodecs>()
            .expect("registered")
    }

    #[test]
    fn json_payload_encodes_as_the_postcard_payload() {
        let kind = codecs()
            .encode(&header(true), "core.move", json!({"target": [200, 100.5]}))
            .expect("encodes");
        let expected = MoveCmd {
            target: fl_math::FxVec2::new(
                fl_math::Fx::from_int(200),
                fl_math::Fx::from_f64(100.5).expect("in range"),
            ),
        };
        assert_eq!(kind.type_id.0, 0);
        assert_eq!(
            kind.payload,
            postcard::to_allocvec(&expected).expect("encodes")
        );
    }

    #[test]
    fn every_order_and_debug_command_has_a_codec() {
        let names: Vec<&str> = codecs().names().collect();
        assert_eq!(
            names,
            [
                "core.follow",
                "core.move",
                "core.rotate",
                "core.stop",
                "debug.invulnerable",
                "debug.set_hp",
                "debug.set_owner",
                "debug.set_terrain",
                "debug.spawn",
                "debug.teleport",
                "debug.wander",
            ]
        );
    }

    #[test]
    fn bad_names_payloads_and_cheats_are_errors() {
        let codecs = codecs();
        let unknown = codecs
            .encode(&header(true), "core.fly", json!(null))
            .expect_err("unknown");
        assert!(unknown.contains("'core.fly'"), "{unknown}");
        let bad = codecs
            .encode(
                &header(true),
                "core.move",
                json!({"target": [1, 2], "x": 1}),
            )
            .expect_err("unknown field");
        assert!(bad.contains("core.move"), "{bad}");
        let cheat = codecs
            .encode(&header(false), "debug.set_hp", json!({"hp": 3}))
            .expect_err("cheats off");
        assert!(cheat.contains("cheats"), "{cheat}");
        let untabled = codecs
            .encode(&header(true), "core.stop", json!(null))
            .expect_err("not in the table");
        assert!(untabled.contains("table"), "{untabled}");
    }
}
