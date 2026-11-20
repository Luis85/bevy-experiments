use std::collections::BTreeSet;
use terra_core::{GeoPosition, LocalFrame};
use terra_sandbox::*;

fn origin() -> GeoPosition {
    GeoPosition::new(48.137, 11.575, 520.).unwrap()
}

fn offset(east: f64, north: f64) -> GeoPosition {
    GeoPosition {
        altitude_m: 520.,
        ..LocalFrame::new(origin())
            .to_geo([east, 0., -north])
            .unwrap()
    }
}

fn apply(state: &mut SandboxState, player: u64, command: Command, catalog: &Catalog) -> Outcome {
    state.apply(player, command, catalog).unwrap()
}

fn setup() -> (SandboxState, Catalog, u64) {
    let mut state = SandboxState::new();
    let catalog = Catalog::builtin();
    for id in 1..=4 {
        state.add_player(id, origin()).unwrap();
    }
    let faction = apply(
        &mut state,
        1,
        Command::CreateFaction {
            name: "River Commons".into(),
        },
        &catalog,
    )
    .created_id
    .unwrap();
    for player in [2, 3] {
        join(&mut state, 1, faction, player, &catalog);
    }
    (state, catalog, faction)
}

fn join(state: &mut SandboxState, leader: u64, faction: u64, player: u64, catalog: &Catalog) {
    apply(
        state,
        leader,
        Command::InviteToFaction {
            faction_id: faction,
            player_id: player,
        },
        catalog,
    );
    apply(
        state,
        player,
        Command::AcceptFactionInvite {
            faction_id: faction,
        },
        catalog,
    );
}

fn house(state: &mut SandboxState, player: u64, position: GeoPosition, catalog: &Catalog) -> u64 {
    apply(
        state,
        player,
        Command::Craft {
            recipe: "planks".into(),
            count: 3,
        },
        catalog,
    );
    apply(
        state,
        player,
        Command::Craft {
            recipe: "stone_blocks".into(),
            count: 4,
        },
        catalog,
    );
    apply(
        state,
        player,
        Command::Build {
            blueprint: "house".into(),
            position,
        },
        catalog,
    )
    .created_id
    .unwrap()
}

fn shared_assets(state: &mut SandboxState, faction: u64, catalog: &Catalog) -> (u64, u64, u64) {
    let plot = apply(
        state,
        1,
        Command::ClaimPlot {
            center: origin(),
            radius_m: 30.,
        },
        catalog,
    )
    .created_id
    .unwrap();
    let home = house(state, 1, offset(8., 0.), catalog);
    let farm = apply(
        state,
        1,
        Command::Plant {
            plot_id: plot,
            position: origin(),
            crop: "wheat".into(),
        },
        catalog,
    )
    .created_id
    .unwrap();
    apply(
        state,
        1,
        Command::TransferPlotToFaction {
            plot_id: plot,
            faction_id: faction,
        },
        catalog,
    );
    for player in [1, 2] {
        apply(
            state,
            player,
            Command::SetHome { structure_id: home },
            catalog,
        );
    }
    (plot, home, farm)
}

#[test]
fn lifecycle_rejections_are_atomic_and_never_create_a_leaderless_faction() {
    let (mut state, catalog, faction) = setup();
    apply(
        &mut state,
        1,
        Command::InviteToFaction {
            faction_id: faction,
            player_id: 4,
        },
        &catalog,
    );
    let before = state.clone();
    for (caller, command) in [
        (
            1,
            Command::LeaveFaction {
                faction_id: faction,
            },
        ),
        (
            1,
            Command::RemoveFactionMember {
                faction_id: faction,
                player_id: 1,
            },
        ),
        (
            2,
            Command::RemoveFactionMember {
                faction_id: faction,
                player_id: 3,
            },
        ),
        (
            2,
            Command::TransferFactionLeadership {
                faction_id: faction,
                player_id: 2,
            },
        ),
        (
            1,
            Command::TransferFactionLeadership {
                faction_id: faction,
                player_id: 1,
            },
        ),
        (
            1,
            Command::TransferFactionLeadership {
                faction_id: faction,
                player_id: 4,
            },
        ),
        (
            1,
            Command::TransferFactionLeadership {
                faction_id: faction,
                player_id: 999,
            },
        ),
        (
            4,
            Command::LeaveFaction {
                faction_id: faction,
            },
        ),
        (
            1,
            Command::RemoveFactionMember {
                faction_id: faction,
                player_id: 4,
            },
        ),
        (
            2,
            Command::RevokeFactionInvite {
                faction_id: faction,
                player_id: 4,
            },
        ),
        (
            1,
            Command::DeclineFactionInvite {
                faction_id: faction,
            },
        ),
        (
            1,
            Command::RevokeFactionInvite {
                faction_id: faction,
                player_id: 3,
            },
        ),
        (2, Command::LeaveFaction { faction_id: 999 }),
    ] {
        assert!(state.apply(caller, command, &catalog).is_err());
        assert_eq!(
            state, before,
            "rejected lifecycle actions must not change any authority, home or balance"
        );
    }
    state.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn leaving_revokes_shared_home_and_permissions_but_preserves_every_asset() {
    let (mut state, catalog, faction) = setup();
    let (plot, home, farm) = shared_assets(&mut state, faction, &catalog);
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
    let mut expected = state.clone();
    expected
        .factions
        .get_mut(&faction)
        .unwrap()
        .members
        .remove(&2);
    expected.players.get_mut(&2).unwrap().home = None;
    let outcome = apply(
        &mut state,
        2,
        Command::LeaveFaction {
            faction_id: faction,
        },
        &catalog,
    );
    assert!(outcome.message.contains("shared home unset"));
    assert_eq!(state, expected);
    assert_eq!(state.plots[&plot].faction_owner, Some(faction));
    assert_eq!(state.structures[&home].faction_owner, Some(faction));
    assert_eq!(state.farms[&farm].faction_owner, Some(faction));
    assert_eq!(state.players[&1].home, Some(home));
    assert!(!state.may_build(2, Some(faction), 2));
    assert!(!state.may_use(2, Some(faction), 2));
    for command in [
        Command::SetHome { structure_id: home },
        Command::Damage {
            structure_id: home,
            amount: 1,
        },
        Command::AcceptFactionInvite {
            faction_id: faction,
        },
    ] {
        assert!(state.apply(2, command, &catalog).is_err());
        assert_eq!(state, expected);
    }
    join(&mut state, 1, faction, 2, &catalog);
    assert_eq!(state.factions[&faction].members[&2], FactionRole::Member);
    assert_eq!(
        state.players[&2].home, None,
        "rejoining does not silently reclaim a home or the previous role"
    );
    state.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn leadership_handover_retains_invitations_and_old_leader_has_no_roster_bypass() {
    let (mut state, catalog, faction) = setup();
    let (_, home, _) = shared_assets(&mut state, faction, &catalog);
    apply(
        &mut state,
        1,
        Command::InviteToFaction {
            faction_id: faction,
            player_id: 4,
        },
        &catalog,
    );
    let mut expected = state.clone();
    let roster = expected.factions.get_mut(&faction).unwrap();
    roster.leader = 2;
    roster.members.insert(1, FactionRole::Builder);
    roster.members.insert(2, FactionRole::Leader);
    apply(
        &mut state,
        1,
        Command::TransferFactionLeadership {
            faction_id: faction,
            player_id: 2,
        },
        &catalog,
    );
    assert_eq!(state, expected);
    assert!(state.may_build(1, Some(faction), 1));
    assert!(state.factions[&faction].invites.contains(&4));
    for command in [
        Command::RemoveFactionMember {
            faction_id: faction,
            player_id: 3,
        },
        Command::RevokeFactionInvite {
            faction_id: faction,
            player_id: 4,
        },
        Command::SetFactionRole {
            faction_id: faction,
            player_id: 3,
            role: FactionRole::Builder,
        },
        Command::TransferFactionLeadership {
            faction_id: faction,
            player_id: 1,
        },
    ] {
        assert!(state.apply(1, command, &catalog).is_err());
        assert_eq!(state, expected);
    }
    apply(
        &mut state,
        2,
        Command::RemoveFactionMember {
            faction_id: faction,
            player_id: 1,
        },
        &catalog,
    );
    assert_eq!(state.players[&1].home, None);
    assert_eq!(state.players[&2].home, Some(home));
    assert_eq!(
        state.structures[&home].owner, 1,
        "founder remains audit identity"
    );
    assert!(
        !state.may_use(1, Some(faction), 1),
        "original owner cannot bypass revocation"
    );
    assert!(state.may_build(2, Some(faction), 1));
    apply(
        &mut state,
        2,
        Command::TransferFactionLeadership {
            faction_id: faction,
            player_id: 3,
        },
        &catalog,
    );
    apply(
        &mut state,
        2,
        Command::LeaveFaction {
            faction_id: faction,
        },
        &catalog,
    );
    assert_eq!(
        state.factions[&faction].members,
        [(3, FactionRole::Leader)].into_iter().collect()
    );
    let before = state.clone();
    assert!(
        state
            .apply(
                3,
                Command::LeaveFaction {
                    faction_id: faction
                },
                &catalog
            )
            .is_err()
    );
    assert_eq!(state, before);
    state.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn invitation_decline_and_revocation_release_capacity_without_implicitly_joining() {
    let mut state = SandboxState::new();
    let catalog = Catalog::builtin();
    state.add_player(1, origin()).unwrap();
    state.add_player(2, origin()).unwrap();
    let mut factions = Vec::new();
    for index in 0..MAX_PLAYER_FACTIONS {
        let faction = apply(
            &mut state,
            1,
            Command::CreateFaction {
                name: format!("Community {index}"),
            },
            &catalog,
        )
        .created_id
        .unwrap();
        apply(
            &mut state,
            1,
            Command::InviteToFaction {
                faction_id: faction,
                player_id: 2,
            },
            &catalog,
        );
        factions.push(faction);
    }
    let before = state.clone();
    assert!(matches!(
        state.apply(
            2,
            Command::CreateFaction {
                name: "Own community".into()
            },
            &catalog
        ),
        Err(SandboxError::Capacity)
    ));
    assert_eq!(state, before);
    apply(
        &mut state,
        2,
        Command::DeclineFactionInvite {
            faction_id: factions[0],
        },
        &catalog,
    );
    assert!(!state.factions[&factions[0]].invites.contains(&2));
    assert!(!state.factions[&factions[0]].members.contains_key(&2));
    apply(
        &mut state,
        2,
        Command::CreateFaction {
            name: "Own community".into(),
        },
        &catalog,
    );
    state.set_player_dormant(2, true).unwrap();
    apply(
        &mut state,
        1,
        Command::RevokeFactionInvite {
            faction_id: factions[1],
            player_id: 2,
        },
        &catalog,
    );
    assert!(!state.factions[&factions[1]].invites.contains(&2));
    for faction in &factions[2..] {
        assert!(state.factions[faction].invites.contains(&2));
    }
    state.set_player_dormant(2, false).unwrap();
    let before = state.clone();
    assert!(
        state
            .apply(
                2,
                Command::AcceptFactionInvite {
                    faction_id: factions[1]
                },
                &catalog
            )
            .is_err()
    );
    assert_eq!(state, before);
    state.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn leaving_preserves_personal_homes_and_other_memberships() {
    let (mut state, catalog, faction) = setup();
    let place = offset(120., 0.);
    state.set_player_position(2, place).unwrap();
    apply(
        &mut state,
        2,
        Command::ClaimPlot {
            center: place,
            radius_m: 20.,
        },
        &catalog,
    );
    let home = house(&mut state, 2, place, &catalog);
    apply(
        &mut state,
        2,
        Command::SetHome { structure_id: home },
        &catalog,
    );
    let other = apply(
        &mut state,
        2,
        Command::CreateFaction {
            name: "Other community".into(),
        },
        &catalog,
    )
    .created_id
    .unwrap();
    let before = state.clone();
    apply(
        &mut state,
        2,
        Command::LeaveFaction {
            faction_id: faction,
        },
        &catalog,
    );
    assert_eq!(state.players, before.players);
    assert_eq!(state.structures, before.structures);
    assert_eq!(state.factions[&other], before.factions[&other]);
    assert_eq!(state.players[&2].home, Some(home));
    state.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn removing_an_offline_member_clears_cold_home_and_survives_both_regional_merge_paths() {
    let (mut state, catalog, faction) = setup();
    let (plot, home, farm) = shared_assets(&mut state, faction, &catalog);
    let remote = GeoPosition::new(50.0, 14.0, 520.).unwrap();
    state.plots.get_mut(&plot).unwrap().center = remote;
    state.structures.get_mut(&home).unwrap().position = remote;
    state.farms.get_mut(&farm).unwrap().position = remote;
    state.players.get_mut(&1).unwrap().home = None;
    state.set_player_position(2, remote).unwrap();
    state.set_player_dormant(2, true).unwrap();
    let partition = ValidatedRegionPartition::from_state(&state, 15, &catalog).unwrap();
    let mut working = partition.select(&BTreeSet::new()).unwrap();
    assert!(!working.state.structures.contains_key(&home));
    assert!(working.state.is_partial());
    let reservations = working.state.dormant_reservations().clone();
    let before = working.state.clone();
    assert!(
        working
            .state
            .apply(
                2,
                Command::LeaveFaction {
                    faction_id: faction
                },
                &catalog
            )
            .is_err(),
        "offline principal cannot issue its own commands"
    );
    assert_eq!(working.state, before);
    apply(
        &mut working.state,
        1,
        Command::RemoveFactionMember {
            faction_id: faction,
            player_id: 2,
        },
        &catalog,
    );
    assert_eq!(working.state.players[&2].home, None);
    assert_eq!(
        *working.state.dormant_reservations(),
        reservations,
        "cold reservations stay tied to the previous working set until merge"
    );
    working.state.validate_with_catalog(&catalog).unwrap();
    let scoped = partition
        .merge(&working.state, &working.active_regions)
        .unwrap()
        .assemble()
        .unwrap();
    let full = partition
        .merge_validating_all(&working.state, &working.active_regions)
        .unwrap()
        .assemble()
        .unwrap();
    assert_eq!(scoped, full);
    let mut expected = state.clone();
    expected
        .factions
        .get_mut(&faction)
        .unwrap()
        .members
        .remove(&2);
    expected.players.get_mut(&2).unwrap().home = None;
    assert_eq!(scoped, expected);
    let path = std::env::temp_dir().join(format!("terra-faction-cold-{}.json", std::process::id()));
    scoped.save(&path).unwrap();
    let mut resumed = SandboxState::load(&path).unwrap();
    resumed.set_player_dormant(2, false).unwrap();
    assert_eq!(
        resumed.players[&2].position, remote,
        "membership revocation does not teleport an offline body"
    );
    assert_eq!(resumed.structures[&home], state.structures[&home]);
    assert_eq!(resumed.players[&2].home, None);
    assert!(
        resumed
            .apply(2, Command::SetHome { structure_id: home }, &catalog)
            .is_err()
    );
    resumed.validate_with_catalog(&catalog).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn handover_and_departure_preserve_public_funds_stock_and_private_economy_on_resume() {
    let (mut state, catalog, faction) = setup();
    state.seed_living_demo(origin(), &[], &catalog).unwrap();
    let territory = apply(
        &mut state,
        1,
        Command::Settlement {
            action: SettlementCommand::Found {
                name: "River neighborhood".into(),
                center: origin(),
                faction_owner: Some(faction),
            },
        },
        &catalog,
    )
    .created_id
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
        Command::Settlement {
            action: SettlementCommand::Contribute {
                settlement_id: territory,
                item: "planks".into(),
                amount: 3,
            },
        },
        &catalog,
    );
    apply(
        &mut state,
        1,
        Command::Settlement {
            action: SettlementCommand::Fund {
                settlement_id: territory,
                amount: 2500,
            },
        },
        &catalog,
    );
    let before = state.clone();
    state.set_player_dormant(2, true).unwrap();
    apply(
        &mut state,
        1,
        Command::TransferFactionLeadership {
            faction_id: faction,
            player_id: 2,
        },
        &catalog,
    );
    assert!(
        state.may_build(1, Some(faction), 1),
        "outgoing leader can maintain public works while successor is offline"
    );
    apply(
        &mut state,
        1,
        Command::LeaveFaction {
            faction_id: faction,
        },
        &catalog,
    );
    assert_eq!(state.settlements, before.settlements);
    assert_eq!(state.players, before.players);
    assert_eq!(state.living, before.living);
    assert_eq!(state.settlements.territories[&territory].treasury, 7500);
    assert_eq!(state.settlements.territories[&territory].stock["planks"], 3);
    assert_eq!(state.settlements.territories[&territory].founder, 1);
    let path = std::env::temp_dir().join(format!(
        "terra-faction-successor-{}.json",
        std::process::id()
    ));
    state.save(&path).unwrap();
    let mut resumed = SandboxState::load(&path).unwrap();
    assert_eq!(state, resumed);
    resumed.set_player_dormant(2, false).unwrap();
    resumed.set_player_position(2, origin()).unwrap();
    let before = resumed.clone();
    assert!(
        resumed
            .apply(
                1,
                Command::Settlement {
                    action: SettlementCommand::Fund {
                        settlement_id: territory,
                        amount: 1
                    }
                },
                &catalog
            )
            .is_err()
    );
    assert_eq!(
        resumed, before,
        "audit founder cannot mutate faction public funds after departure"
    );
    apply(
        &mut resumed,
        2,
        Command::Settlement {
            action: SettlementCommand::Fund {
                settlement_id: territory,
                amount: 1,
            },
        },
        &catalog,
    );
    assert_eq!(resumed.settlements.territories[&territory].treasury, 7501);
    assert_eq!(
        resumed.living.accounts[&2].money + 1,
        before.living.accounts[&2].money
    );
    resumed.validate_with_catalog(&catalog).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn lifecycle_intents_roundtrip_internally_tagged_wire_envelopes() {
    #[derive(serde::Serialize, serde::Deserialize)]
    #[serde(tag = "type", rename_all = "snake_case")]
    enum Envelope {
        Sandbox { sequence: u64, command: Command },
    }
    for command in [
        Command::DeclineFactionInvite { faction_id: 11 },
        Command::RevokeFactionInvite {
            faction_id: 11,
            player_id: 2,
        },
        Command::LeaveFaction { faction_id: 11 },
        Command::DissolveFaction { faction_id: 11 },
        Command::RemoveFactionMember {
            faction_id: 11,
            player_id: 2,
        },
        Command::TransferFactionLeadership {
            faction_id: 11,
            player_id: 2,
        },
    ] {
        let before = serde_json::to_value(Envelope::Sandbox {
            sequence: 9,
            command,
        })
        .unwrap();
        let parsed: Envelope = serde_json::from_value(before.clone()).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), before);
    }
}
