use terra_core::{GeoPosition, LocalFrame, WeatherSample};
use terra_sandbox::*;

#[test]
fn faction_rosters_roundtrip_inside_internally_tagged_network_envelopes() {
    #[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    #[serde(tag = "type", rename_all = "snake_case")]
    enum Message {
        Snapshot { factions: Vec<Faction> },
    }
    let faction = Faction {
        id: 9,
        name: "Cooperative".into(),
        leader: 1,
        members: [(1, FactionRole::Leader), (2, FactionRole::Builder)]
            .into_iter()
            .collect(),
        invites: [3].into_iter().collect(),
    };
    let message = Message::Snapshot {
        factions: vec![faction],
    };
    let json = serde_json::to_vec(&message).unwrap();
    assert_eq!(serde_json::from_slice::<Message>(&json).unwrap(), message);
    let malformed = r#"{"type":"snapshot","factions":[{"id":9,"name":"Cooperative","leader":1,"members":{"1":"leader","01":"member"},"invites":[]}] }"#;
    assert!(serde_json::from_str::<Message>(malformed).is_err());
}

fn position() -> GeoPosition {
    GeoPosition::new(48.137, 11.575, 520.).unwrap()
}
fn offset(east: f64, north: f64) -> GeoPosition {
    GeoPosition {
        altitude_m: 520.,
        ..LocalFrame::new(position())
            .to_geo([east, 0., -north])
            .unwrap()
    }
}
fn setup() -> (SandboxState, Catalog) {
    let mut state = SandboxState::new();
    for id in 1..=3 {
        state.add_player(id, position()).unwrap();
    }
    (state, Catalog::builtin())
}
fn apply(
    state: &mut SandboxState,
    player: u64,
    command: Command,
    catalog: &Catalog,
) -> Option<u64> {
    state.apply(player, command, catalog).unwrap().created_id
}
fn faction(state: &mut SandboxState, catalog: &Catalog) -> u64 {
    apply(
        state,
        1,
        Command::CreateFaction {
            name: "Northern Commons".into(),
        },
        catalog,
    )
    .unwrap()
}
fn join(state: &mut SandboxState, id: u64, member: u64, catalog: &Catalog) {
    apply(
        state,
        1,
        Command::InviteToFaction {
            faction_id: id,
            player_id: member,
        },
        catalog,
    );
    apply(
        state,
        member,
        Command::AcceptFactionInvite { faction_id: id },
        catalog,
    );
}

#[test]
fn invitations_and_role_grants_prevent_self_join_and_escalation() {
    let (mut state, catalog) = setup();
    let faction = faction(&mut state, &catalog);
    let before = state.clone();
    assert!(
        state
            .apply(
                2,
                Command::AcceptFactionInvite {
                    faction_id: faction
                },
                &catalog
            )
            .is_err()
    );
    assert_eq!(state, before);
    assert!(
        state
            .apply(
                2,
                Command::InviteToFaction {
                    faction_id: faction,
                    player_id: 3
                },
                &catalog
            )
            .is_err()
    );
    assert_eq!(state, before);
    join(&mut state, faction, 2, &catalog);
    assert_eq!(state.factions[&faction].members[&2], FactionRole::Member);
    let before = state.clone();
    for (caller, target, role) in [
        (2, 2, FactionRole::Builder),
        (1, 2, FactionRole::Leader),
        (1, 1, FactionRole::Member),
    ] {
        assert!(
            state
                .apply(
                    caller,
                    Command::SetFactionRole {
                        faction_id: faction,
                        player_id: target,
                        role
                    },
                    &catalog
                )
                .is_err()
        );
        assert_eq!(state, before);
    }
    state.validate().unwrap();
}

#[test]
fn transferring_land_also_transfers_assets_and_roles_control_shared_mutation() {
    let (mut state, catalog) = setup();
    let plot = apply(
        &mut state,
        1,
        Command::ClaimPlot {
            center: position(),
            radius_m: 30.,
        },
        &catalog,
    )
    .unwrap();
    apply(
        &mut state,
        1,
        Command::Craft {
            recipe: "planks".into(),
            count: 3,
        },
        &catalog,
    );
    apply(
        &mut state,
        1,
        Command::Craft {
            recipe: "stone_blocks".into(),
            count: 4,
        },
        &catalog,
    );
    let house = apply(
        &mut state,
        1,
        Command::Build {
            blueprint: "house".into(),
            position: offset(8., 0.),
        },
        &catalog,
    )
    .unwrap();
    let crop = apply(
        &mut state,
        1,
        Command::Plant {
            plot_id: plot,
            position: position(),
            crop: "wheat".into(),
        },
        &catalog,
    )
    .unwrap();
    let faction = faction(&mut state, &catalog);
    join(&mut state, faction, 2, &catalog);
    apply(
        &mut state,
        1,
        Command::TransferPlotToFaction {
            plot_id: plot,
            faction_id: faction,
        },
        &catalog,
    );
    assert_eq!(state.plots[&plot].faction_owner, Some(faction));
    assert_eq!(state.structures[&house].faction_owner, Some(faction));
    assert_eq!(state.farms[&crop].faction_owner, Some(faction));
    let before = state.clone();
    assert!(matches!(
        state.apply(
            2,
            Command::Build {
                blueprint: "fence".into(),
                position: offset(-8., 0.)
            },
            &catalog
        ),
        Err(SandboxError::NotOwner)
    ));
    assert_eq!(state, before);
    apply(
        &mut state,
        1,
        Command::SetFactionRole {
            faction_id: faction,
            player_id: 2,
            role: FactionRole::Builder,
        },
        &catalog,
    );
    apply(
        &mut state,
        2,
        Command::Craft {
            recipe: "planks".into(),
            count: 1,
        },
        &catalog,
    );
    let fence = apply(
        &mut state,
        2,
        Command::Build {
            blueprint: "fence".into(),
            position: offset(-8., 0.),
        },
        &catalog,
    )
    .unwrap();
    assert_eq!(state.structures[&fence].owner, 2);
    assert_eq!(state.structures[&fence].faction_owner, Some(faction));
    state
        .advance(
            1000.,
            &WeatherSample {
                temperature_c: 20.,
                ..Default::default()
            },
        )
        .unwrap();
    apply(&mut state, 2, Command::Harvest { farm_id: crop }, &catalog);
    assert_eq!(state.players[&2].inventory["wheat"], 4);
    apply(
        &mut state,
        2,
        Command::Damage {
            structure_id: house,
            amount: 25,
        },
        &catalog,
    );
    assert_eq!(state.structures[&house].hitpoints, 175);
    apply(
        &mut state,
        1,
        Command::SetFactionRole {
            faction_id: faction,
            player_id: 2,
            role: FactionRole::Member,
        },
        &catalog,
    );
    apply(
        &mut state,
        2,
        Command::SetHome {
            structure_id: house,
        },
        &catalog,
    );
    let before = state.clone();
    // Creator ID cannot bypass a role downgrade on faction property.
    assert!(
        state
            .apply(
                2,
                Command::Damage {
                    structure_id: fence,
                    amount: 25
                },
                &catalog
            )
            .is_err()
    );
    assert_eq!(state, before);
    assert!(
        state
            .apply(
                3,
                Command::SetHome {
                    structure_id: house
                },
                &catalog
            )
            .is_err()
    );
    assert_eq!(state, before);
    state.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn plot_transfer_cannot_expropriate_another_players_property() {
    let (mut state, catalog) = setup();
    let faction = faction(&mut state, &catalog);
    let plot = apply(
        &mut state,
        2,
        Command::ClaimPlot {
            center: position(),
            radius_m: 20.,
        },
        &catalog,
    )
    .unwrap();
    let before = state.clone();
    assert!(
        state
            .apply(
                1,
                Command::TransferPlotToFaction {
                    plot_id: plot,
                    faction_id: faction
                },
                &catalog
            )
            .is_err()
    );
    assert_eq!(state, before);
    assert!(
        state
            .apply(
                2,
                Command::TransferPlotToFaction {
                    plot_id: plot,
                    faction_id: faction
                },
                &catalog
            )
            .is_err()
    );
    assert_eq!(state, before);
}

#[test]
fn faction_persistence_rejects_invalid_rosters_and_retains_shared_ownership() {
    let (mut state, catalog) = setup();
    let faction = faction(&mut state, &catalog);
    join(&mut state, faction, 2, &catalog);
    let plot = apply(
        &mut state,
        1,
        Command::ClaimPlot {
            center: position(),
            radius_m: 20.,
        },
        &catalog,
    )
    .unwrap();
    apply(
        &mut state,
        1,
        Command::TransferPlotToFaction {
            plot_id: plot,
            faction_id: faction,
        },
        &catalog,
    );
    let path = std::env::temp_dir().join(format!("terra-factions-{}.json", std::process::id()));
    state.save(&path).unwrap();
    assert_eq!(SandboxState::load(&path).unwrap(), state);
    state
        .factions
        .get_mut(&faction)
        .unwrap()
        .members
        .insert(999, FactionRole::Builder);
    assert!(state.validate().is_err());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn faction_snapshot_prioritizes_memberships_and_reports_truncation() {
    let (mut state, catalog) = setup();
    for index in 0..8 {
        apply(
            &mut state,
            1,
            Command::CreateFaction {
                name: format!("Group {index}"),
            },
            &catalog,
        );
    }
    let before = state.clone();
    assert!(
        state
            .apply(
                1,
                Command::CreateFaction {
                    name: "Ninth group".into()
                },
                &catalog
            )
            .is_err()
    );
    assert_eq!(state, before);
    let other = apply(
        &mut state,
        2,
        Command::CreateFaction {
            name: "Neighbors".into(),
        },
        &catalog,
    )
    .unwrap();
    state.set_player_position(2, offset(50., 0.)).unwrap();
    let plot = apply(
        &mut state,
        2,
        Command::ClaimPlot {
            center: offset(50., 0.),
            radius_m: 10.,
        },
        &catalog,
    )
    .unwrap();
    apply(
        &mut state,
        2,
        Command::TransferPlotToFaction {
            plot_id: plot,
            faction_id: other,
        },
        &catalog,
    );
    let view = state.snapshot_for(1, 100.).unwrap();
    assert_eq!(view.factions.len(), 8);
    assert!(view.truncated);
    assert!(view.factions.iter().all(|f| f.members.contains_key(&1)));
    assert!(serde_json::to_vec(&view).unwrap().len() < 256 * 1024);
    state.validate().unwrap();
}

#[test]
fn additive_faction_fields_keep_personal_v1_saves_readable() {
    let (mut state, catalog) = setup();
    let plot = apply(
        &mut state,
        1,
        Command::ClaimPlot {
            center: position(),
            radius_m: 20.,
        },
        &catalog,
    )
    .unwrap();
    let mut legacy = serde_json::to_value(&state).unwrap();
    legacy.as_object_mut().unwrap().remove("factions");
    legacy["plots"][plot.to_string()]
        .as_object_mut()
        .unwrap()
        .remove("faction_owner");
    let restored: SandboxState = serde_json::from_value(legacy).unwrap();
    restored.validate().unwrap();
    assert!(restored.factions.is_empty());
    assert_eq!(restored.plots[&plot].faction_owner, None);
}
