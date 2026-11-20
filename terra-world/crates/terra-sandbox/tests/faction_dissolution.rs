//! Dissolving an unused community cannot reclaim assets or erase obligations.
use std::collections::BTreeSet;
use terra_core::{GeoPosition, LocalFrame};
use terra_sandbox::*;

fn origin() -> GeoPosition {
    GeoPosition::new(48.137, 11.575, 100.).unwrap()
}

fn fixture() -> (SandboxState, Catalog) {
    let mut state = SandboxState::new();
    for player in 1..=4 {
        state.add_player(player, origin()).unwrap();
    }
    (state, Catalog::builtin())
}

fn apply(state: &mut SandboxState, player: u64, action: Command, catalog: &Catalog) -> u64 {
    state
        .apply(player, action, catalog)
        .unwrap()
        .created_id
        .unwrap_or(0)
}

fn create(state: &mut SandboxState, player: u64, name: &str, catalog: &Catalog) -> u64 {
    apply(
        state,
        player,
        Command::CreateFaction { name: name.into() },
        catalog,
    )
}

fn reject(state: &mut SandboxState, player: u64, faction: u64, catalog: &Catalog) {
    let before = state.clone();
    assert!(
        state
            .faction_dissolution_readiness(player, faction)
            .is_err()
    );
    assert_eq!(*state, before, "readiness must be read-only");
    assert!(
        state
            .apply(
                player,
                Command::DissolveFaction {
                    faction_id: faction
                },
                catalog,
            )
            .is_err()
    );
    assert_eq!(*state, before, "rejection must preserve the entire world");
}

fn property(
    state: &mut SandboxState,
    owner: u64,
    faction: Option<u64>,
    position: GeoPosition,
    catalog: &Catalog,
) -> (u64, u64, u64) {
    state.set_player_position(owner, position).unwrap();
    let plot = apply(
        state,
        owner,
        Command::ClaimPlot {
            center: position,
            radius_m: 30.,
        },
        catalog,
    );
    for (recipe, count) in [("planks", 3), ("stone_blocks", 4)] {
        apply(
            state,
            owner,
            Command::Craft {
                recipe: recipe.into(),
                count,
            },
            catalog,
        );
    }
    let house = apply(
        state,
        owner,
        Command::Build {
            blueprint: "house".into(),
            position: LocalFrame::new(position).to_geo([8., 0., 0.]).unwrap(),
        },
        catalog,
    );
    let farm = apply(
        state,
        owner,
        Command::Plant {
            plot_id: plot,
            position,
            crop: "wheat".into(),
        },
        catalog,
    );
    if let Some(faction_id) = faction {
        apply(
            state,
            owner,
            Command::TransferPlotToFaction {
                plot_id: plot,
                faction_id,
            },
            catalog,
        );
    }
    (plot, house, farm)
}

#[test]
fn sole_leader_dissolves_only_the_faction_and_frees_invitation_slots_without_id_reuse() {
    let (mut state, catalog) = fixture();
    state.seed_living_demo(origin(), &[], &catalog).unwrap();
    state.advance_settlements(60., &catalog).unwrap();
    let mut factions = Vec::new();
    for index in 0..MAX_PLAYER_FACTIONS {
        let faction = create(&mut state, 1, &format!("Draft {index}"), &catalog);
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
    let other = create(&mut state, 3, "Established elsewhere", &catalog);
    state.set_player_dormant(2, true).unwrap();
    let before = state.clone();
    for player in [1, 2] {
        let mut active = state.clone();
        active.set_player_dormant(player, false).unwrap();
        assert!(
            active
                .apply(
                    player,
                    Command::CreateFaction {
                        name: "No free affiliation".into(),
                    },
                    &catalog,
                )
                .is_err()
        );
    }
    let removed = factions[0];
    assert!(state.faction_dissolution_readiness(1, removed).is_ok());
    assert_eq!(state, before);
    let outcome = state
        .apply(
            1,
            Command::DissolveFaction {
                faction_id: removed,
            },
            &catalog,
        )
        .unwrap();
    assert!(outcome.message.contains("1 invitations revoked"));
    let mut expected = before.clone();
    expected.factions.remove(&removed);
    assert_eq!(
        state, expected,
        "no goods, money, needs, jobs or IDs change"
    );
    assert_eq!(state.factions[&other], before.factions[&other]);
    assert!(state.is_player_dormant(2));
    assert!(
        state
            .snapshot_for(2, 100.)
            .unwrap()
            .factions
            .iter()
            .all(|faction| faction.id != removed)
    );
    reject(&mut state, 1, removed, &catalog);

    let next = state.next_id;
    let replacement = create(&mut state, 1, "Intentional replacement", &catalog);
    assert_eq!(replacement, next);
    assert!(replacement > removed);
    state.set_player_dormant(2, false).unwrap();
    create(&mut state, 2, "Former invitee's choice", &catalog);
    state.validate_with_catalog(&catalog).unwrap();

    let path = std::env::temp_dir().join(format!(
        "terra-faction-dissolution-resume-{}.json",
        std::process::id()
    ));
    state.save(&path).unwrap();
    let resumed = SandboxState::load_with_catalog(&path, &catalog).unwrap();
    assert_eq!(resumed, state);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn dissolution_rejects_outsiders_invitees_members_missing_ids_and_dormant_leaders_atomically() {
    let (mut state, catalog) = fixture();
    let faction = create(&mut state, 1, "Keep the leader", &catalog);
    apply(
        &mut state,
        1,
        Command::InviteToFaction {
            faction_id: faction,
            player_id: 2,
        },
        &catalog,
    );
    reject(&mut state, 2, faction, &catalog);
    reject(&mut state, 3, faction, &catalog);
    reject(&mut state, 999, faction, &catalog);
    reject(&mut state, 1, 999, &catalog);
    state.set_player_dormant(1, true).unwrap();
    reject(&mut state, 1, faction, &catalog);
    state.set_player_dormant(1, false).unwrap();
    apply(
        &mut state,
        2,
        Command::AcceptFactionInvite {
            faction_id: faction,
        },
        &catalog,
    );
    reject(&mut state, 1, faction, &catalog);
    reject(&mut state, 2, faction, &catalog);
    state.set_player_dormant(2, true).unwrap();
    reject(&mut state, 1, faction, &catalog);
    state.validate_with_catalog(&catalog).unwrap();
}

#[test]
fn global_land_protects_cold_shared_houses_and_crops_from_dissolution() {
    let (mut state, catalog) = fixture();
    let faction = create(&mut state, 1, "Remote commons", &catalog);
    let remote = GeoPosition::new(50., 14., 100.).unwrap();
    let (plot, house, farm) = property(&mut state, 1, Some(faction), remote, &catalog);
    state.set_player_position(1, origin()).unwrap();
    state.validate_with_catalog(&catalog).unwrap();
    reject(&mut state, 1, faction, &catalog);
    let partition = ValidatedRegionPartition::from_state(&state, 15, &catalog).unwrap();
    let mut working = partition.select(&BTreeSet::new()).unwrap();
    assert!(working.state.is_partial());
    assert!(working.state.plots.contains_key(&plot));
    assert!(!working.state.structures.contains_key(&house));
    assert!(!working.state.farms.contains_key(&farm));
    reject(&mut working.state, 1, faction, &catalog);
    for merged in [
        partition
            .merge(&working.state, &working.active_regions)
            .unwrap(),
        partition
            .merge_validating_all(&working.state, &working.active_regions)
            .unwrap(),
    ] {
        assert_eq!(merged.assemble().unwrap(), state);
    }
}

#[test]
fn a_shared_territory_blocks_dissolution_with_or_without_public_funds_and_stock() {
    let (mut state, catalog) = fixture();
    state.seed_living_demo(origin(), &[], &catalog).unwrap();
    let faction = create(&mut state, 1, "Public works", &catalog);
    let territory = apply(
        &mut state,
        1,
        Command::Settlement {
            action: SettlementCommand::Found {
                name: "Kept neighborhood".into(),
                center: origin(),
                faction_owner: Some(faction),
            },
        },
        &catalog,
    );
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
    state
        .set_player_position(1, GeoPosition::new(50., 14., 100.).unwrap())
        .unwrap();
    assert_eq!(state.settlements.territories[&territory].treasury, 5000);
    assert_eq!(state.settlements.territories[&territory].stock["planks"], 3);
    reject(&mut state, 1, faction, &catalog);
    // A host-authored zero-balance territory still owns territory; dissolution
    // is not a back door to erase public jurisdiction once its funds are spent.
    let shared = state.settlements.territories.get_mut(&territory).unwrap();
    shared.treasury = 0;
    shared.stock.clear();
    state.validate_with_catalog(&catalog).unwrap();
    reject(&mut state, 1, faction, &catalog);
}

#[test]
fn unrelated_cold_property_and_offline_home_survive_dissolution_and_both_merges() {
    let (mut state, catalog) = fixture();
    let empty = create(&mut state, 1, "Unused idea", &catalog);
    let other = create(&mut state, 2, "Retained community", &catalog);
    let remote = GeoPosition::new(50., 14., 100.).unwrap();
    let (_, house, farm) = property(&mut state, 2, Some(other), remote, &catalog);
    apply(
        &mut state,
        2,
        Command::SetHome {
            structure_id: house,
        },
        &catalog,
    );
    state.set_player_dormant(2, true).unwrap();
    let partition = ValidatedRegionPartition::from_state(&state, 15, &catalog).unwrap();
    let mut working = partition.select(&BTreeSet::new()).unwrap();
    assert!(!working.state.structures.contains_key(&house));
    assert!(!working.state.farms.contains_key(&farm));
    assert!(working.state.is_partial());
    let reservations = working.state.dormant_reservations().clone();
    apply(
        &mut working.state,
        1,
        Command::DissolveFaction { faction_id: empty },
        &catalog,
    );
    assert_eq!(*working.state.dormant_reservations(), reservations);
    let mut expected = state;
    expected.factions.remove(&empty);
    for merged in [
        partition
            .merge(&working.state, &working.active_regions)
            .unwrap(),
        partition
            .merge_validating_all(&working.state, &working.active_regions)
            .unwrap(),
    ] {
        assert_eq!(merged.assemble().unwrap(), expected);
    }
}
