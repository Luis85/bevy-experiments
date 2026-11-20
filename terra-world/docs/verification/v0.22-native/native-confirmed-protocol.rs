use serde::{Deserialize, Serialize};
use terra_core::{ActorState, GeoPosition};
use terra_sandbox::{Command, Outcome, SandboxView};

/// Increment whenever the wire contract changes incompatibly.
pub const PROTOCOL_VERSION: u16 = 11;

/// Immutable base-world identity is independent from replicated mutable actors.
/// Games should publish a versioned pack hash in `base_data_revision` before
/// relying on identical collision/navigation geometry on different machines.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BaseWorldManifest {
    pub world_id: String,
    pub seed: u64,
    pub base_data_revision: String,
    pub terrain_provider: String,
    pub chunk_level: u8,
    pub terrain_resolution: usize,
    /// Timestamp of an explicitly versioned geographic dataset, not game time.
    #[serde(default)]
    pub data_epoch_unix: Option<i64>,
    /// Independent, versioned political overlay identity when supplied.
    #[serde(default)]
    pub political_dataset_revision: Option<String>,
}

impl Default for BaseWorldManifest {
    fn default() -> Self {
        Self {
            world_id: "terra-reference-world".into(),
            seed: 42,
            base_data_revision: "terra-procedural-v2".into(),
            terrain_provider: "procedural".into(),
            chunk_level: 15,
            terrain_resolution: 65,
            data_epoch_unix: None,
            political_dataset_revision: None,
        }
    }
}

impl BaseWorldManifest {
    /// Source compatibility is independent from the host's save/session name.
    /// A client may join different worlds built from exactly the same source data.
    pub fn matches_data(&self, expected: &Self) -> bool {
        self.seed == expected.seed
            && self.base_data_revision == expected.base_data_revision
            && self.terrain_provider == expected.terrain_provider
            && self.chunk_level == expected.chunk_level
            && self.terrain_resolution == expected.terrain_resolution
            && self.data_epoch_unix == expected.data_epoch_unix
            && self.political_dataset_revision == expected.political_dataset_revision
    }
}

/// Movement is intent only: a client never submits authoritative coordinates.
/// ENU velocity is `[east, north, up]`, in meters per real simulation second.
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ClientMessage {
    Hello {
        protocol: u16,
        interest: GeoPosition,
        /// Capability returned by an earlier Welcome; transport must protect it.
        #[serde(default)]
        resume_token: Option<String>,
        /// Validate source identity before allocating a saved player. None is
        /// reserved for inspection clients that can consume any server world.
        #[serde(default)]
        expected_manifest: Option<BaseWorldManifest>,
    },
    Move {
        sequence: u64,
        velocity_enu_m_s: [f64; 3],
    },
    /// Registered road vehicles accept pedal/steering intent. Their velocity,
    /// fuel, damage and geodetic position are integrated by the authority.
    Drive {
        sequence: u64,
        input: terra_core::DriveInput,
    },
    /// Change spectator interest; this never moves the owned actor.
    Focus {
        position: GeoPosition,
        radius_m: f64,
    },
    /// Sequence space is independent from movement; replay is rejected.
    Sandbox {
        sequence: u64,
        command: Command,
    },
    /// Request fresh replication baselines; does not change gameplay state.
    Resync {},
    Ping {
        nonce: u64,
    },
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ServerMessage {
    /// Private authoritative pose and input acknowledgement for local preview.
    MotionState {
        state: crate::prediction::MotionSnapshot,
    },
    /// Wire-only revisioned updates. NetClient normalizes these to snapshots.
    ActorUpdate {
        update: crate::replication::ActorUpdate,
    },
    SandboxUpdate {
        update: crate::replication::SandboxUpdate,
    },
    Welcome {
        protocol: u16,
        client_id: u64,
        actor_id: u64,
        manifest: BaseWorldManifest,
        tick: u64,
        unix_seconds: f64,
        time_scale: f64,
        paused: bool,
        tick_hz: u32,
        spawn: GeoPosition,
        resume_token: String,
        next_sandbox_sequence: u64,
    },
    Snapshot {
        tick: u64,
        unix_seconds: f64,
        actors: Vec<ActorState>,
        /// True when the configured per-client actor budget was reached.
        truncated: bool,
        total_in_interest: usize,
    },
    Pong {
        nonce: u64,
        tick: u64,
    },
    SandboxResult {
        sequence: u64,
        outcome: Outcome,
    },
    SandboxRejected {
        sequence: u64,
        message: String,
    },
    SandboxSnapshot {
        tick: u64,
        view: SandboxView,
    },
    Error {
        code: String,
        message: String,
    },
}

impl ServerMessage {
    /// Validate the shared application handshake before binding a replica or
    /// retaining its bearer capability. Source/catalog policy remains game-owned.
    pub fn validate_welcome(&self) -> crate::Result<()> {
        let Self::Welcome {
            protocol,
            client_id,
            actor_id,
            spawn,
            unix_seconds,
            time_scale,
            tick_hz,
            resume_token,
            next_sandbox_sequence,
            ..
        } = self
        else {
            return Err(crate::NetError::Protocol("expected server welcome".into()));
        };
        if *protocol != PROTOCOL_VERSION
            || *client_id == 0
            || *actor_id == 0
            || !validate_position(*spawn)
            || !unix_seconds.is_finite()
            || unix_seconds.abs() > 1e12
            || !time_scale.is_finite()
            || !(0.0..=86_400.0).contains(time_scale)
            || !(1..=120).contains(tick_hz)
            || *next_sandbox_sequence == u64::MAX
            || resume_token.len() != 64
            || !resume_token.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(crate::NetError::Protocol("invalid server welcome".into()));
        }
        Ok(())
    }
}

// Debug is intentionally independent of serde: wire messages must carry the
// capability, while routine diagnostics and assertion failures must not print it.
impl std::fmt::Debug for ClientMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hello {
                protocol,
                interest,
                resume_token,
                expected_manifest,
            } => f
                .debug_struct("Hello")
                .field("protocol", protocol)
                .field("interest", interest)
                .field("resume_token", &resume_token.as_ref().map(|_| "[REDACTED]"))
                .field("expected_manifest", expected_manifest)
                .finish(),
            Self::Move {
                sequence,
                velocity_enu_m_s,
            } => f
                .debug_struct("Move")
                .field("sequence", sequence)
                .field("velocity_enu_m_s", velocity_enu_m_s)
                .finish(),
            Self::Drive { sequence, input } => f
                .debug_struct("Drive")
                .field("sequence", sequence)
                .field("input", input)
                .finish(),
            Self::Focus { position, radius_m } => f
                .debug_struct("Focus")
                .field("position", position)
                .field("radius_m", radius_m)
                .finish(),
            Self::Sandbox { sequence, command } => f
                .debug_struct("Sandbox")
                .field("sequence", sequence)
                .field("command", command)
                .finish(),
            Self::Resync {} => f.debug_struct("Resync").finish(),
            Self::Ping { nonce } => f.debug_struct("Ping").field("nonce", nonce).finish(),
        }
    }
}

impl std::fmt::Debug for ServerMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MotionState { state } => {
                f.debug_struct("MotionState").field("state", state).finish()
            }
            Self::ActorUpdate { update } => f
                .debug_struct("ActorUpdate")
                .field("update", update)
                .finish(),
            Self::SandboxUpdate { update } => f
                .debug_struct("SandboxUpdate")
                .field("update", update)
                .finish(),
            Self::Welcome {
                protocol,
                client_id,
                actor_id,
                manifest,
                tick,
                unix_seconds,
                time_scale,
                paused,
                tick_hz,
                spawn,
                next_sandbox_sequence,
                resume_token: _,
            } => f
                .debug_struct("Welcome")
                .field("protocol", protocol)
                .field("client_id", client_id)
                .field("actor_id", actor_id)
                .field("manifest", manifest)
                .field("tick", tick)
                .field("unix_seconds", unix_seconds)
                .field("time_scale", time_scale)
                .field("paused", paused)
                .field("tick_hz", tick_hz)
                .field("spawn", spawn)
                .field("resume_token", &"[REDACTED]")
                .field("next_sandbox_sequence", next_sandbox_sequence)
                .finish(),
            Self::Snapshot {
                tick,
                unix_seconds,
                actors,
                truncated,
                total_in_interest,
            } => f
                .debug_struct("Snapshot")
                .field("tick", tick)
                .field("unix_seconds", unix_seconds)
                .field("actors", actors)
                .field("truncated", truncated)
                .field("total_in_interest", total_in_interest)
                .finish(),
            Self::Pong { nonce, tick } => f
                .debug_struct("Pong")
                .field("nonce", nonce)
                .field("tick", tick)
                .finish(),
            Self::SandboxResult { sequence, outcome } => f
                .debug_struct("SandboxResult")
                .field("sequence", sequence)
                .field("outcome", outcome)
                .finish(),
            Self::SandboxRejected { sequence, message } => f
                .debug_struct("SandboxRejected")
                .field("sequence", sequence)
                .field("message", message)
                .finish(),
            Self::SandboxSnapshot { tick, view } => f
                .debug_struct("SandboxSnapshot")
                .field("tick", tick)
                .field("view", view)
                .finish(),
            Self::Error { code, message } => f
                .debug_struct("Error")
                .field("code", code)
                .field("message", message)
                .finish(),
        }
    }
}

/// Great-circle distance on the WGS84 mean-radius sphere, used for interest
/// filtering only. Geodetic actor positions retain their full f64 precision.
pub fn surface_distance_m(a: GeoPosition, b: GeoPosition) -> f64 {
    let lat_a = a.latitude.to_radians();
    let lat_b = b.latitude.to_radians();
    let d_lat = lat_b - lat_a;
    let d_lon = (b.longitude - a.longitude).to_radians();
    let h = ((d_lat / 2.0).sin().powi(2) + lat_a.cos() * lat_b.cos() * (d_lon / 2.0).sin().powi(2))
        .clamp(0.0, 1.0);
    2.0 * 6_371_008.8 * h.sqrt().asin()
}

pub(crate) fn validate_position(position: GeoPosition) -> bool {
    position.latitude.is_finite()
        && position.longitude.is_finite()
        && position.altitude_m.is_finite()
        && (-90.0..=90.0).contains(&position.latitude)
        && (-180.0..=180.0).contains(&position.longitude)
        && (-12_000.0..=100_000.0).contains(&position.altitude_m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn welcome_admission_rejects_nonfinite_clocks_before_capability_use() {
        let good = ServerMessage::Welcome {
            protocol: PROTOCOL_VERSION,
            client_id: 1,
            actor_id: 1,
            manifest: BaseWorldManifest::default(),
            tick: 0,
            unix_seconds: 0.,
            time_scale: 0.,
            paused: true,
            tick_hz: 1,
            spawn: GeoPosition::new(0., 0., 0.).unwrap(),
            resume_token: "A".repeat(64),
            next_sandbox_sequence: 0,
        };
        assert!(good.validate_welcome().is_ok());
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            for clock in [true, false] {
                let mut bad = good.clone();
                if let ServerMessage::Welcome {
                    unix_seconds,
                    time_scale,
                    ..
                } = &mut bad
                {
                    if clock {
                        *unix_seconds = value;
                    } else {
                        *time_scale = value;
                    }
                }
                assert!(bad.validate_welcome().is_err());
            }
        }
        assert!(
            ServerMessage::Pong { nonce: 1, tick: 0 }
                .validate_welcome()
                .is_err()
        );
    }

    #[test]
    fn capability_debug_redaction_preserves_private_wire_handshakes() {
        let token = "a1b2c3d4".repeat(8);
        let position = GeoPosition::new(48.137, 11.575, 0.).unwrap();
        let hello = ClientMessage::Hello {
            protocol: PROTOCOL_VERSION,
            interest: position,
            resume_token: Some(token.clone()),
            expected_manifest: None,
        };
        let welcome = ServerMessage::Welcome {
            protocol: PROTOCOL_VERSION,
            client_id: 7,
            actor_id: 11,
            manifest: BaseWorldManifest::default(),
            tick: 3,
            unix_seconds: 100.,
            time_scale: 60.,
            paused: false,
            tick_hz: 20,
            spawn: position,
            resume_token: token.clone(),
            next_sandbox_sequence: 5,
        };
        // Nested aggregates exercise the same formatting used by diagnostics
        // and assertion failures, including their alternate/pretty form.
        for rendered in [
            format!("{hello:?}"),
            format!("{hello:#?}"),
            format!("{welcome:?}"),
            format!("{welcome:#?}"),
            format!("{:?}", (&hello, vec![&welcome])),
            format!("{:#?}", (&hello, vec![&welcome])),
        ] {
            assert!(!rendered.contains(&token));
            assert!(!rendered.contains(&token[..8]));
            assert!(rendered.contains("[REDACTED]"));
            assert!(rendered.contains("resume_token"));
        }
        assert!(format!("{hello:?}").contains("Hello"));
        assert!(format!("{welcome:?}").contains("actor_id: 11"));
        let guest = ClientMessage::Hello {
            protocol: PROTOCOL_VERSION,
            interest: position,
            resume_token: None,
            expected_manifest: None,
        };
        for rendered in [format!("{guest:?}"), format!("{guest:#?}")] {
            assert!(rendered.contains("resume_token: None"));
            assert!(!rendered.contains("[REDACTED]"));
        }

        let expected_hello = serde_json::json!({
            "type": "hello", "protocol": 10, "interest": position,
            "resume_token": token, "expected_manifest": null,
        });
        let expected_welcome = serde_json::json!({
            "type": "welcome", "protocol": 10, "client_id": 7, "actor_id": 11,
            "manifest": BaseWorldManifest::default(), "tick": 3,
            "unix_seconds": 100., "time_scale": 60., "paused": false,
            "tick_hz": 20, "spawn": position, "resume_token": token,
            "next_sandbox_sequence": 5,
        });
        assert_eq!(serde_json::to_value(&hello).unwrap(), expected_hello);
        assert_eq!(serde_json::to_value(&welcome).unwrap(), expected_welcome);
        let decoded_hello: ClientMessage = serde_json::from_value(expected_hello.clone()).unwrap();
        let decoded_welcome: ServerMessage =
            serde_json::from_value(expected_welcome.clone()).unwrap();
        assert_eq!(serde_json::to_value(decoded_hello).unwrap(), expected_hello);
        assert_eq!(
            serde_json::to_value(decoded_welcome).unwrap(),
            expected_welcome
        );
    }

    #[test]
    fn interest_distance_wraps_the_dateline() {
        let a = GeoPosition::new(0.0, 179.999, 0.0).unwrap();
        let b = GeoPosition::new(0.0, -179.999, 0.0).unwrap();
        let distance = surface_distance_m(a, b);
        assert!((222.0..223.0).contains(&distance));
    }

    #[test]
    fn positions_from_deserialization_still_require_validation() {
        let outside_globe: GeoPosition =
            serde_json::from_str(r#"{"latitude":100,"longitude":0,"altitude_m":0}"#).unwrap();
        assert!(!validate_position(outside_globe));
    }

    #[test]
    fn unknown_command_fields_are_rejected() {
        assert!(
            serde_json::from_str::<ClientMessage>(
                r#"{"type":"move","sequence":1,"velocity_enu_m_s":[1,0,0],"actor_id":999}"#
            )
            .is_err()
        );
    }

    #[test]
    fn maximum_bounded_sandbox_view_fits_a_single_frame() {
        use std::collections::{BTreeMap, BTreeSet};
        use terra_sandbox::{
            Faction, FactionRole, FarmPlot, LandPlot, PlayerState, ResourceNode, Structure,
        };
        let position =
            GeoPosition::new(48.137123456789, 11.575123456789, 1234.567890123456).unwrap();
        let id = |number: usize| format!("{number:020}{}", "x".repeat(108));
        // Use maximum-width numeric identifiers and every bounded collection.
        // Faction rosters are deliberately conservative: both lists are full.
        let player_ids: Vec<u64> = (0..128).map(|number| u64::MAX - number).collect();
        let members: BTreeMap<_, _> = player_ids
            .iter()
            .map(|id| (*id, FactionRole::Builder))
            .collect();
        let invites: BTreeSet<_> = player_ids.iter().copied().collect();
        let view = SandboxView {
            transport: terra_sandbox::TransportView::default(),
            ecology: terra_sandbox::EcologyView::default(),
            settlements: terra_sandbox::SettlementView::default(),
            living: terra_sandbox::LivingView::default(),
            version: terra_sandbox::SAVE_VERSION,
            tick_seconds: 999999999999.1234,
            player: PlayerState {
                id: u64::MAX,
                position,
                home: Some(u64::MAX),
                inventory: (0..64).map(|number| (id(number), 1_000_000)).collect(),
            },
            factions: (0..8)
                .map(|number| Faction {
                    id: u64::MAX - number,
                    name: "é".repeat(64),
                    leader: u64::MAX,
                    members: members.clone(),
                    invites: invites.clone(),
                })
                .collect(),
            plots: (0..64)
                .map(|number| LandPlot {
                    id: u64::MAX - number,
                    owner: u64::MAX,
                    faction_owner: Some(u64::MAX),
                    center: position,
                    radius_m: 99.12345678912345,
                })
                .collect(),
            structures: (0..64)
                .map(|number| Structure {
                    id: u64::MAX - number as u64,
                    owner: u64::MAX,
                    faction_owner: Some(u64::MAX),
                    blueprint: id(number),
                    position,
                    hitpoints: 1_000_000,
                    max_hitpoints: 1_000_000,
                    radius_m: 49.12345678912345,
                    housing: true,
                    workbench: true,
                })
                .collect(),
            resource_nodes: (0..64)
                .map(|number| ResourceNode {
                    id: id(number),
                    position,
                    item: id(number),
                    remaining: 10000,
                    capacity: 10000,
                    regrow_seconds: 31535999.12345678,
                    regrow_accumulator: 12345678.12345678,
                    source_feature: Some(id(number)),
                })
                .collect(),
            farms: (0..64)
                .map(|number| FarmPlot {
                    id: u64::MAX - number as u64,
                    owner: u64::MAX,
                    faction_owner: Some(u64::MAX),
                    plot_id: u64::MAX,
                    position,
                    crop: id(number),
                    growth_seconds: 12345678.12345678,
                    required_growth_seconds: 31535999.12345678,
                    moisture: 0.12345678,
                    harvest_item: id(number),
                    harvest_amount: 10000,
                })
                .collect(),
            feature_edits: (0..64).map(id).collect(),
            truncated: true,
        };
        let frame = crate::encode_frame(&ServerMessage::SandboxSnapshot {
            tick: u64::MAX,
            view: view.clone(),
        })
        .unwrap();
        assert!(
            frame.len() < crate::MAX_FRAME_BYTES,
            "bounded view unexpectedly uses {} bytes",
            frame.len()
        );
        // The new economy shares the same frame, including pathological long
        // item IDs. Its bounded reduction retains active/return bodies.
        let mut state = terra_sandbox::SandboxState::default();
        state.add_player(u64::MAX, position).unwrap();
        state
            .seed_living_demo(position, &[], &terra_sandbox::Catalog::builtin())
            .unwrap();
        let mut living = state.snapshot_for(u64::MAX, 5000.).unwrap().living;
        let base_unit = living.units[0].clone();
        while living.units.len() < 64 {
            let mut unit = base_unit.clone();
            unit.id = 1_300_000 + living.units.len() as u64;
            unit.name = "é".repeat(32);
            unit.owner = None;
            living.units.push(unit);
        }
        let mut shop = living.shops[0].clone();
        shop.stock = (0..64).map(|n| (id(n), 1_000_000)).collect();
        shop.prices = (0..64)
            .map(|n| {
                (
                    id(n),
                    terra_sandbox::TradePrice {
                        buy: 1_000_000_000,
                        sell: 1_000_000_000,
                    },
                )
            })
            .collect();
        living.shops = (0..8)
            .map(|n| {
                let mut s = shop.clone();
                s.id = 1_100_000 + n;
                s.name = "é".repeat(32);
                s
            })
            .collect();
        let mut site = living.producers[0].clone();
        site.stock = shop.stock.clone();
        site.inputs = shop.stock.clone();
        site.outputs = shop.stock.clone();
        living.producers = (0..16)
            .map(|n| {
                let mut s = site.clone();
                s.id = 1_200_000 + n;
                s.name = "é".repeat(32);
                s
            })
            .collect();
        let mut combined = view;
        combined.living = living;
        combined.enforce_wire_budget().unwrap();
        combined.living.validate_for(u64::MAX).unwrap();
        assert!(combined.truncated);
        let frame = crate::encode_frame(&ServerMessage::SandboxSnapshot {
            tick: u64::MAX,
            view: combined,
        })
        .unwrap();
        assert!(frame.len() < crate::MAX_FRAME_BYTES);
    }
}
