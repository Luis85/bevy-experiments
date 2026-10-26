use serde::{Deserialize, Serialize};
use terra_core::{ActorState, GeoPosition};
use terra_sandbox::{Command, Outcome, SandboxView};

/// Increment whenever the wire contract changes incompatibly.
pub const PROTOCOL_VERSION: u16 = 1;

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
#[derive(Clone, Debug, Serialize, Deserialize)]
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
    Ping {
        nonce: u64,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
#[expect(
    clippy::large_enum_variant,
    reason = "the bounded snapshot stays inline to preserve the public message construction API"
)]
pub enum ServerMessage {
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
