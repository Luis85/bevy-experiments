use terra_core::{GeoPosition, LocalFrame};
use terra_sandbox::*;

fn world() -> (SandboxState, Catalog, GeoPosition) {
    let catalog = Catalog::builtin();
    let origin = GeoPosition::new(48.137, 11.575, 100.).unwrap();
    let mut state = SandboxState::default();
    state.add_player(1, origin).unwrap();
    state.add_player(2, origin).unwrap();
    state.seed_living_demo(origin, &[], &catalog).unwrap();
    state.advance_settlements(0., &catalog).unwrap();
    (state, catalog, origin)
}

fn apply_guide(state: &mut SandboxState, catalog: &Catalog, guide: ActivityGuidance) {
    let Some(GuideAction::Command(command)) = guide.action else {
        panic!("expected a command: {guide:?}");
    };
    state.apply(1, command, catalog).unwrap();
}

#[test]
fn guidance_is_read_only_for_all_tracks_and_missing_principals() {
    let (state, catalog, _) = world();
    let before = state.clone();
    for player in [1, 2, 999] {
        for track in [
            GuideTrack::Suggested,
            GuideTrack::Supply,
            GuideTrack::Home,
            GuideTrack::Community,
            GuideTrack::Road,
            GuideTrack::Company,
        ] {
            let guide = state.activity_guidance(player, &catalog, track, None);
            assert!(!guide.detail.is_empty());
            assert!(!guide.action_label.is_empty());
            if player == 999 {
                assert!(guide.action.is_none());
            }
        }
    }
    assert_eq!(state, before);
}

#[test]
fn suggested_critical_water_precedes_work_and_food_and_uses_real_inventory() {
    let (mut state, catalog, _) = world();
    let offered = state.activity_guidance(1, &catalog, GuideTrack::Supply, None);
    apply_guide(&mut state, &catalog, offered);
    let contract = state
        .settlements
        .contracts
        .values()
        .find(|c| c.claimed_by == Some(1))
        .unwrap()
        .clone();
    let active = state.living.accounts[&1].active_unit;
    state.living.units.get_mut(&active).unwrap().needs = Needs {
        water: 0.2,
        food: 0.,
        energy: 0.1,
    };
    state
        .players
        .get_mut(&1)
        .unwrap()
        .inventory
        .insert("water".into(), 2);
    state
        .players
        .get_mut(&1)
        .unwrap()
        .inventory
        .insert("bread".into(), 1);
    let wallet = state.living.accounts[&1].money;
    let before = state.clone();
    let urgent = state.activity_guidance(1, &catalog, GuideTrack::Suggested, None);
    assert_eq!(state, before);
    assert_eq!(urgent.track, GuideTrack::Suggested);
    assert!(
        matches!(&urgent.action, Some(GuideAction::Command(Command::Living {
        action: LivingCommand::Consume { item }
    })) if item == "water")
    );
    apply_guide(&mut state, &catalog, urgent);
    assert_eq!(state.players[&1].inventory["water"], 1);
    assert!((state.living.units[&active].needs.water - 0.8).abs() < 1e-6);
    let food = state.activity_guidance(1, &catalog, GuideTrack::Suggested, None);
    assert!(
        matches!(&food.action, Some(GuideAction::Command(Command::Living {
        action: LivingCommand::Consume { item }
    })) if item == "bread")
    );
    apply_guide(&mut state, &catalog, food);
    assert_eq!(state.living.accounts[&1].money, wallet);
    assert_eq!(state.settlements.contracts[&contract.id], contract);
    assert_eq!(state.settlements.progress[&1].contracts_completed, 0);
}

#[test]
fn registered_vehicle_guidance_reads_driver_avatar_and_only_opens_exit_controls() {
    let (mut state, catalog, _) = world();
    state.ensure_transport_demo(&catalog).unwrap();
    let vehicle = state
        .transport
        .vehicles
        .values()
        .find(|v| v.owner == 1)
        .unwrap()
        .unit_id;
    // This two-principal fixture seeds both starter rovers at the same offset.
    // Driver-needs guidance needs an ordinary clear parking place; overlapping
    // another owner's vehicle is covered by the dismount rejection regressions.
    state.living.units.get_mut(&vehicle).unwrap().position =
        LocalFrame::new(state.living.units[&vehicle].position)
            .to_geo([300., 0., 300.])
            .unwrap();
    state
        .set_player_position(1, state.living.units[&vehicle].position)
        .unwrap();
    state
        .apply(
            1,
            Command::Transport {
                action: TransportCommand::EnterVehicle {
                    vehicle_id: vehicle,
                },
            },
            &catalog,
        )
        .unwrap();
    let avatar = state.living.accounts[&1].avatar_unit;
    for (verb, needs) in [
        (
            "drink",
            Needs {
                water: 0.,
                ..Default::default()
            },
        ),
        (
            "eat",
            Needs {
                food: 0.1,
                ..Default::default()
            },
        ),
        (
            "rest",
            Needs {
                energy: 0.1,
                ..Default::default()
            },
        ),
    ] {
        state.living.units.get_mut(&avatar).unwrap().needs = needs;
        let before = state.clone();
        let guide = state.activity_guidance(1, &catalog, GuideTrack::Suggested, None);
        assert_eq!(guide.title, format!("Park and exit to {verb}"));
        assert!(matches!(
            guide.action,
            Some(GuideAction::Open(GuideSurface::Driving))
        ));
        assert_eq!(state, before);
    }
    // A vehicle's own placeholder needs are not its driver's condition.
    state.living.units.get_mut(&avatar).unwrap().needs = Needs::default();
    state.living.units.get_mut(&vehicle).unwrap().needs.water = 0.;
    assert_ne!(
        state
            .activity_guidance(1, &catalog, GuideTrack::Suggested, None)
            .track,
        GuideTrack::Suggested
    );
    state.living.units.get_mut(&avatar).unwrap().needs.water = 0.;
    state
        .players
        .get_mut(&1)
        .unwrap()
        .inventory
        .insert("water".into(), 1);
    state
        .apply(
            1,
            Command::Transport {
                action: TransportCommand::ExitVehicle,
            },
            &catalog,
        )
        .unwrap();
    let on_foot = state.activity_guidance(1, &catalog, GuideTrack::Suggested, None);
    assert!(
        matches!(&on_foot.action, Some(GuideAction::Command(Command::Living {
        action: LivingCommand::Consume { item }
    })) if item == "water")
    );
}

#[test]
fn urgent_water_without_supplies_uses_existing_gather_and_consume_then_bounded_fallback() {
    let (mut state, catalog, origin) = world();
    let active = state.living.accounts[&1].active_unit;
    state.living.units.get_mut(&active).unwrap().needs.water = 0.1;
    state.players.get_mut(&1).unwrap().inventory.remove("water");
    state.resource_nodes.clear();
    state
        .seed_resource("nearby-water".into(), origin, "water", 3, 0.)
        .unwrap();
    let before = state.clone();
    let acquire = state.activity_guidance(1, &catalog, GuideTrack::Suggested, None);
    assert!(
        matches!(&acquire.action, Some(GuideAction::Command(Command::Gather { node_id, amount: 1 })) if node_id == "nearby-water")
    );
    assert_eq!(state, before);
    apply_guide(&mut state, &catalog, acquire);
    assert_eq!(state.resource_nodes["nearby-water"].remaining, 2);
    assert_eq!(state.players[&1].inventory["water"], 1);
    let drink = state.activity_guidance(1, &catalog, GuideTrack::Suggested, None);
    apply_guide(&mut state, &catalog, drink);
    assert!(state.living.units[&active].needs.water > 0.6);
    assert_eq!(
        state.players[&1]
            .inventory
            .get("water")
            .copied()
            .unwrap_or(0),
        0
    );
    assert_eq!(
        state.living.accounts[&1].money,
        before.living.accounts[&1].money
    );
    state.living.units.get_mut(&active).unwrap().needs.water = 0.1;
    state.resource_nodes.clear();
    state.living.shops.clear();
    let before = state.clone();
    let unavailable = state.activity_guidance(1, &catalog, GuideTrack::Suggested, None);
    assert!(matches!(
        unavailable.action,
        Some(GuideAction::Open(GuideSurface::Nature))
    ));
    assert!(unavailable.detail.contains("nearest 128"));
    assert_eq!(state, before);
}

#[test]
fn urgent_food_uses_only_carried_supported_consumables_for_the_active_person() {
    for item in ["bread", "cooked_fish", "berries"] {
        let (mut state, catalog, _) = world();
        let avatar = state.living.accounts[&1].avatar_unit;
        let crew = state
            .living
            .units
            .values()
            .find(|unit| unit.owner == Some(1) && unit.is_person() && unit.id != avatar)
            .unwrap()
            .id;
        state
            .apply(
                1,
                Command::Living {
                    action: LivingCommand::SelectActive { unit_id: crew },
                },
                &catalog,
            )
            .unwrap();
        state.living.units.get_mut(&crew).unwrap().needs.food = 0.1;
        state.players.get_mut(&1).unwrap().inventory.clear();
        state
            .players
            .get_mut(&1)
            .unwrap()
            .inventory
            .insert(item.into(), 1);
        state
            .players
            .get_mut(&1)
            .unwrap()
            .inventory
            .insert("fish".into(), 1);
        let guide = state.activity_guidance(1, &catalog, GuideTrack::Suggested, None);
        assert!(
            matches!(&guide.action, Some(GuideAction::Command(Command::Living {
            action: LivingCommand::Consume { item: chosen }
        })) if chosen == item)
        );
        apply_guide(&mut state, &catalog, guide);
        assert!(state.living.units[&crew].needs.food > 0.2);
        assert_eq!(state.living.units[&avatar].needs.food, 1.);
        assert_eq!(state.players[&1].inventory["fish"], 1);
        assert_eq!(
            state.players[&1].inventory.get(item).copied().unwrap_or(0),
            0
        );
    }
}

#[test]
fn urgent_needs_preserve_explicit_tracks_and_read_only_save_resume() {
    let (mut state, catalog, _) = world();
    let explicit: Vec<_> = [
        GuideTrack::Supply,
        GuideTrack::Home,
        GuideTrack::Community,
        GuideTrack::Road,
    ]
    .into_iter()
    .map(|track| (track, state.activity_guidance(1, &catalog, track, None)))
    .collect();
    let active = state.living.accounts[&1].active_unit;
    state.living.units.get_mut(&active).unwrap().needs = Needs {
        water: 0.,
        food: 0.,
        energy: 0.,
    };
    let before = state.clone();
    for (track, shown) in explicit {
        let current = state.activity_guidance(1, &catalog, track, None);
        assert_eq!(shown.description(), current.description());
        assert!(shown.action.unwrap().same_intent(&current.action.unwrap()));
    }
    let guide = state.activity_guidance(1, &catalog, GuideTrack::Suggested, None);
    assert_eq!(state, before);
    let restored: SandboxState =
        serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    let resumed = restored.activity_guidance(1, &catalog, GuideTrack::Suggested, None);
    assert_eq!(guide.description(), resumed.description());
    assert!(guide.action.unwrap().same_intent(&resumed.action.unwrap()));
}

#[test]
fn low_energy_only_opens_the_existing_command_controls() {
    let (mut state, catalog, _) = world();
    let active = state.living.accounts[&1].active_unit;
    state.living.units.get_mut(&active).unwrap().needs.energy = 0.1;
    let before = state.clone();
    let guide = state.activity_guidance(1, &catalog, GuideTrack::Suggested, None);
    assert_eq!(guide.title, "Rest before continuing");
    assert!(matches!(
        guide.action,
        Some(GuideAction::Open(GuideSurface::Command))
    ));
    assert_eq!(state, before);
    state.living.units.get_mut(&active).unwrap().order = UnitOrder::Rest;
    let guide = state.activity_guidance(1, &catalog, GuideTrack::Suggested, None);
    assert_eq!(guide.title, "Recover your energy");
    assert!(matches!(
        guide.action,
        Some(GuideAction::Open(GuideSurface::Command))
    ));
}

#[test]
fn supply_journey_uses_real_escrow_inventory_and_once_only_progress() {
    let (mut state, catalog, _) = world();
    let contract = state
        .settlements
        .contracts
        .values()
        .find(|c| state.living.producers.contains_key(&c.target_site))
        .unwrap()
        .clone();
    let id = contract.id;
    let offered = state.activity_guidance(1, &catalog, GuideTrack::Supply, Some(id));
    assert_eq!(offered.step, 1);
    let wallet = state.living.accounts[&1].money;
    apply_guide(&mut state, &catalog, offered);
    assert_eq!(state.living.accounts[&1].money, wallet);
    state
        .players
        .get_mut(&1)
        .unwrap()
        .inventory
        .remove(&contract.item);
    assert_eq!(
        state
            .activity_guidance(1, &catalog, GuideTrack::Supply, Some(id))
            .step,
        2
    );
    state
        .players
        .get_mut(&1)
        .unwrap()
        .inventory
        .insert(contract.item.clone(), contract.amount);
    let far = LocalFrame::new(contract.target_position)
        .to_geo([120., 0., 0.])
        .unwrap();
    state.set_player_position(1, far).unwrap();
    let travel = state.activity_guidance(1, &catalog, GuideTrack::Supply, Some(id));
    assert_eq!(travel.step, 3);
    assert!(matches!(travel.action, Some(GuideAction::Travel { .. })));
    state
        .set_player_position(1, contract.target_position)
        .unwrap();
    let ready = state.activity_guidance(1, &catalog, GuideTrack::Supply, Some(id));
    assert_eq!(ready.step, 4);
    let replay = ready.clone();
    apply_guide(&mut state, &catalog, ready);
    assert_eq!(state.living.accounts[&1].money, wallet + contract.reward);
    assert_eq!(state.settlements.progress[&1].contracts_completed, 1);
    assert_eq!(
        state.players[&1]
            .inventory
            .get(&contract.item)
            .copied()
            .unwrap_or(0),
        0
    );
    let Some(GuideAction::Command(command)) = replay.action else {
        unreachable!()
    };
    let after = state.clone();
    assert!(state.apply(1, command, &catalog).is_err());
    assert_eq!(state, after);
    assert!(
        !matches!(state.activity_guidance(1,&catalog,GuideTrack::Suggested,None).action,Some(GuideAction::Command(Command::Settlement {action:SettlementCommand::FulfillContract {contract_id}})) if contract_id==id)
    );
}

#[test]
fn accepted_work_has_priority_and_other_players_contracts_are_never_suggested() {
    let (mut state, catalog, _) = world();
    let ids: Vec<_> = state.settlements.contracts.keys().copied().collect();
    assert!(ids.len() >= 3);
    state
        .apply(
            1,
            Command::Settlement {
                action: SettlementCommand::AcceptContract {
                    contract_id: ids[0],
                },
            },
            &catalog,
        )
        .unwrap();
    state
        .apply(
            2,
            Command::Settlement {
                action: SettlementCommand::AcceptContract {
                    contract_id: ids[1],
                },
            },
            &catalog,
        )
        .unwrap();
    let a = state.activity_guidance(1, &catalog, GuideTrack::Supply, Some(ids[2]));
    let b = state.activity_guidance(1, &catalog, GuideTrack::Supply, Some(ids[1]));
    let c = state.activity_guidance(1, &catalog, GuideTrack::Supply, Some(ids[0]));
    assert_eq!(a.title, c.title);
    assert_eq!(b.title, c.title);
    assert!(
        a.action
            .as_ref()
            .unwrap()
            .same_intent(c.action.as_ref().unwrap())
    );
}

#[test]
fn guidance_resumes_from_saved_authority_without_a_second_goal_ledger() {
    let (mut state, catalog, _) = world();
    let guide = state.activity_guidance(1, &catalog, GuideTrack::Suggested, None);
    apply_guide(&mut state, &catalog, guide);
    let before = state.activity_guidance(1, &catalog, GuideTrack::Suggested, None);
    let bytes = serde_json::to_vec(&state).unwrap();
    let restored: SandboxState = serde_json::from_slice(&bytes).unwrap();
    let after = restored.activity_guidance(1, &catalog, GuideTrack::Suggested, None);
    assert_eq!(before.description(), after.description());
    assert!(before.action.unwrap().same_intent(&after.action.unwrap()));
}

#[test]
fn acquisition_skips_depleted_and_foreign_protected_resources() {
    let (mut state, catalog, origin) = world();
    let id = *state.settlements.contracts.keys().next().unwrap();
    let c = state.settlements.contracts.get_mut(&id).unwrap();
    c.item = "wood".into();
    c.amount = 5;
    c.claimed_by = Some(1);
    state.players.get_mut(&1).unwrap().inventory.remove("wood");
    state.resource_nodes.clear();
    let frame = LocalFrame::new(origin);
    for (id, offset, remaining) in [("empty", 1., 0), ("private", 30., 5), ("public", 80., 5)] {
        state.resource_nodes.insert(
            id.into(),
            ResourceNode {
                id: id.into(),
                position: frame.to_geo([offset, 0., 0.]).unwrap(),
                item: "wood".into(),
                remaining,
                capacity: 5,
                regrow_seconds: 0.,
                regrow_accumulator: 0.,
                source_feature: None,
            },
        );
    }
    state.plots.insert(
        900,
        LandPlot {
            id: 900,
            owner: 2,
            faction_owner: None,
            center: state.resource_nodes["private"].position,
            radius_m: 10.,
        },
    );
    let guide = state.activity_guidance(1, &catalog, GuideTrack::Supply, Some(id));
    let Some(GuideAction::Travel { position, .. }) = guide.action else {
        panic!("{guide:?}");
    };
    assert_eq!(position, state.resource_nodes["public"].position);
}

#[test]
fn recipe_goals_use_catalog_costs_and_existing_inventory() {
    let (mut state, catalog, _) = world();
    let id = *state.settlements.contracts.keys().next().unwrap();
    let contract = state.settlements.contracts.get_mut(&id).unwrap();
    contract.item = "planks".into();
    contract.amount = 7;
    contract.claimed_by = Some(1);
    state
        .players
        .get_mut(&1)
        .unwrap()
        .inventory
        .insert("planks".into(), 3);
    state
        .players
        .get_mut(&1)
        .unwrap()
        .inventory
        .insert("wood".into(), 2);
    let guide = state.activity_guidance(1, &catalog, GuideTrack::Supply, Some(id));
    assert!(
        matches!(&guide.action,Some(GuideAction::Command(Command::Craft {recipe,count:1})) if recipe=="planks")
    );
    apply_guide(&mut state, &catalog, guide);
    assert_eq!(state.players[&1].inventory["planks"], 7);
    assert!(!state.players[&1].inventory.contains_key("wood"));
}

#[test]
fn custom_recipe_cycles_have_a_bounded_recovery_path() {
    let (mut state, mut catalog, _) = world();
    let id = *state.settlements.contracts.keys().next().unwrap();
    let c = state.settlements.contracts.get_mut(&id).unwrap();
    c.item = "planks".into();
    c.amount = 5;
    c.claimed_by = Some(1);
    state.players.get_mut(&1).unwrap().inventory.clear();
    state.resource_nodes.clear();
    state.living.shops.clear();
    catalog.recipes.get_mut("planks").unwrap().inputs = Inventory::from([("planks".into(), 1)]);
    let guide = state.activity_guidance(1, &catalog, GuideTrack::Supply, Some(id));
    assert!(guide.detail.len() < 2000);
    assert!(matches!(guide.action, Some(GuideAction::Open(_))));
}

#[test]
fn homes_and_territories_respect_current_ownership() {
    let (mut state, catalog, origin) = world();
    state.structures.insert(
        901,
        Structure {
            id: 901,
            owner: 2,
            faction_owner: None,
            blueprint: "house".into(),
            position: origin,
            hitpoints: 200,
            max_hitpoints: 200,
            radius_m: 4.,
            housing: true,
            workbench: false,
        },
    );
    let guide = state.activity_guidance(1, &catalog, GuideTrack::Home, None);
    assert_eq!(guide.step, 1);
    assert!(!matches!(
        guide.action,
        Some(GuideAction::Command(Command::SetHome { .. }))
    ));
    state.structures.get_mut(&901).unwrap().owner = 1;
    let guide = state.activity_guidance(1, &catalog, GuideTrack::Home, None);
    assert!(matches!(
        guide.action,
        Some(GuideAction::Command(Command::SetHome { structure_id: 901 }))
    ));
    apply_guide(&mut state, &catalog, guide);
    assert_eq!(state.players[&1].home, Some(901));
}

#[test]
fn founding_and_funding_explain_real_costs_without_creating_wealth() {
    let (mut state, catalog, _) = world();
    let guide = state.activity_guidance(1, &catalog, GuideTrack::Community, None);
    assert!(matches!(guide.action, Some(GuideAction::NameNeighborhood)));
    state.living.accounts.get_mut(&1).unwrap().money = 4999;
    let guide = state.activity_guidance(1, &catalog, GuideTrack::Community, None);
    assert!(guide.detail.contains("49.99"));
    assert!(matches!(
        guide.action,
        Some(GuideAction::Choose(GuideTrack::Supply))
    ));
}

#[test]
fn changed_transaction_identity_does_not_match_a_visible_action() {
    let a = GuideAction::Command(Command::Settlement {
        action: SettlementCommand::AcceptContract { contract_id: 1 },
    });
    let b = GuideAction::Command(Command::Settlement {
        action: SettlementCommand::AcceptContract { contract_id: 2 },
    });
    assert!(a.same_intent(&a));
    assert!(!a.same_intent(&b));
    assert!(!a.same_intent(&GuideAction::Open(GuideSurface::Market)));
}

#[test]
fn stocked_finished_goods_are_suggested_before_unavailable_recipe_inputs() {
    let (mut state, catalog, origin) = world();
    let id = *state.settlements.contracts.keys().next().unwrap();
    let contract = state.settlements.contracts.get_mut(&id).unwrap();
    contract.item = "planks".into();
    contract.amount = 5;
    contract.claimed_by = Some(1);
    state.players.get_mut(&1).unwrap().inventory.clear();
    state.resource_nodes.clear();
    state.living.accounts.get_mut(&1).unwrap().money = 10000;
    let shop = state.living.shops.values_mut().next().unwrap();
    shop.position = origin;
    shop.name = "Finished timber market".into();
    shop.stock.insert("planks".into(), 20);
    shop.prices
        .insert("planks".into(), TradePrice { buy: 100, sell: 50 });
    let shop_id = shop.id;
    let guide = state.activity_guidance(1, &catalog, GuideTrack::Supply, Some(id));
    assert!(guide.detail.contains("Finished timber market"), "{guide:?}");
    assert!(!guide.detail.contains("first collect"));
    assert!(matches!(
        guide.action,
        Some(GuideAction::OpenMarket { shop_id: id }) if id == shop_id
    ));
}

#[test]
fn urgent_food_chooses_real_available_food_before_a_blocked_bread_recipe() {
    let (mut state, catalog, origin) = world();
    let active = state.living.accounts[&1].active_unit;
    state.living.units.get_mut(&active).unwrap().needs.food = 0.1;
    state.players.get_mut(&1).unwrap().inventory.clear();
    state.resource_nodes.clear();
    for shop in state.living.shops.values_mut() {
        for item in ["bread", "cooked_fish", "berries"] {
            shop.stock.remove(item);
        }
    }
    state
        .seed_resource("ready-berries".into(), origin, "berries", 3, 0.)
        .unwrap();
    let before = state.clone();
    let guide = state.activity_guidance(1, &catalog, GuideTrack::Suggested, None);
    assert_eq!(state, before);
    assert!(guide.title.contains("berries"));
    assert!(
        matches!(&guide.action, Some(GuideAction::Command(Command::Gather {node_id,amount:1})) if node_id == "ready-berries")
    );
    apply_guide(&mut state, &catalog, guide);
    let eat = state.activity_guidance(1, &catalog, GuideTrack::Suggested, None);
    assert!(
        matches!(&eat.action, Some(GuideAction::Command(Command::Living {action:LivingCommand::Consume {item}})) if item == "berries")
    );
    apply_guide(&mut state, &catalog, eat);
    assert!(state.living.units[&active].needs.food > 0.2);
    assert_eq!(state.resource_nodes["ready-berries"].remaining, 2);
    assert_eq!(
        state.living.accounts[&1].money,
        before.living.accounts[&1].money
    );
    assert!(!state.players[&1].inventory.contains_key("berries"));
}

#[test]
fn urgent_food_can_choose_stocked_cooked_fish_without_fabricating_bread_access() {
    let (mut state, catalog, origin) = world();
    let active = state.living.accounts[&1].active_unit;
    state.living.units.get_mut(&active).unwrap().needs.food = 0.;
    state.players.get_mut(&1).unwrap().inventory.clear();
    state.resource_nodes.clear();
    for shop in state.living.shops.values_mut() {
        for item in ["bread", "cooked_fish", "berries"] {
            shop.stock.remove(item);
        }
    }
    let shop = state.living.shops.values_mut().next().unwrap();
    shop.position = origin;
    shop.stock.insert("cooked_fish".into(), 2);
    shop.prices
        .insert("cooked_fish".into(), TradePrice { buy: 100, sell: 50 });
    let seller = shop.id;
    let before = state.clone();
    let guide = state.activity_guidance(1, &catalog, GuideTrack::Suggested, None);
    assert!(guide.title.contains("cooked fish"));
    assert!(matches!(guide.action, Some(GuideAction::OpenMarket {shop_id}) if shop_id == seller));
    assert_eq!(
        state, before,
        "opening recovery must not spend money or create supplies"
    );
}

#[test]
fn held_recipe_ingredients_do_not_hide_a_finished_item_seller_when_workbench_is_unavailable() {
    for distant_bench in [false, true] {
        let (mut state, catalog, origin) = world();
        let id = *state.settlements.contracts.keys().next().unwrap();
        let c = state.settlements.contracts.get_mut(&id).unwrap();
        c.item = "bread".into();
        c.amount = 1;
        c.claimed_by = Some(1);
        state.players.get_mut(&1).unwrap().inventory =
            Inventory::from([("flour".into(), 2), ("water".into(), 1)]);
        state.resource_nodes.clear();
        state.structures.clear();
        if distant_bench {
            state.structures.insert(
                901,
                Structure {
                    id: 901,
                    owner: 1,
                    faction_owner: None,
                    blueprint: "workbench".into(),
                    position: LocalFrame::new(origin).to_geo([150., 0., 0.]).unwrap(),
                    hitpoints: 100,
                    max_hitpoints: 100,
                    radius_m: 1.,
                    housing: false,
                    workbench: true,
                },
            );
        }
        let shop = state.living.shops.values_mut().next().unwrap();
        shop.position = origin;
        shop.stock.insert("bread".into(), 20);
        shop.prices
            .insert("bread".into(), TradePrice { buy: 100, sell: 50 });
        let seller = shop.id;
        let before = state.clone();
        let guide = state.activity_guidance(1, &catalog, GuideTrack::Supply, Some(id));
        assert!(
            matches!(guide.action, Some(GuideAction::OpenMarket {shop_id}) if shop_id == seller)
        );
        assert_eq!(state, before);
    }
}

#[test]
fn overlapping_markets_keep_the_actual_available_seller_and_reject_changed_intent() {
    let (mut state, catalog, origin) = world();
    let id = *state.settlements.contracts.keys().next().unwrap();
    let contract = state.settlements.contracts.get_mut(&id).unwrap();
    contract.item = "planks".into();
    contract.amount = 5;
    contract.claimed_by = Some(1);
    state.players.get_mut(&1).unwrap().inventory.clear();
    state.resource_nodes.clear();
    let original = state.living.shops.values().next().unwrap().clone();
    for shop in state.living.shops.values_mut() {
        shop.stock.remove("planks");
    }
    let near = original.id;
    state.living.shops.get_mut(&near).unwrap().position = origin;
    let mut seller = original;
    seller.id = state.living.next_id;
    state.living.next_id += 1;
    seller.position = LocalFrame::new(origin).to_geo([8., 0., 0.]).unwrap();
    seller.stock.insert("planks".into(), 20);
    seller
        .prices
        .insert("planks".into(), TradePrice { buy: 100, sell: 50 });
    let chosen = seller.id;
    state.living.shops.insert(chosen, seller);
    let first = state.activity_guidance(1, &catalog, GuideTrack::Supply, Some(id));
    assert!(matches!(first.action, Some(GuideAction::OpenMarket {shop_id}) if shop_id == chosen));

    let shop = state.living.shops.get_mut(&near).unwrap();
    shop.stock.insert("planks".into(), 20);
    shop.prices
        .insert("planks".into(), TradePrice { buy: 100, sell: 50 });
    shop.money = MAX_MONEY;
    let still_available = state.activity_guidance(1, &catalog, GuideTrack::Supply, Some(id));
    assert!(
        first
            .action
            .as_ref()
            .unwrap()
            .same_intent(still_available.action.as_ref().unwrap()),
        "a capped nearer till cannot displace the available seller"
    );
    state.living.shops.get_mut(&near).unwrap().money = 0;
    state
        .living
        .shops
        .get_mut(&chosen)
        .unwrap()
        .stock
        .remove("planks");
    let changed = state.activity_guidance(1, &catalog, GuideTrack::Supply, Some(id));
    assert!(matches!(changed.action, Some(GuideAction::OpenMarket {shop_id}) if shop_id == near));
    assert!(
        !first
            .action
            .as_ref()
            .unwrap()
            .same_intent(changed.action.as_ref().unwrap())
    );
}

#[test]
fn resource_permission_work_uses_a_bounded_nearest_shortlist_and_local_refresh() {
    let (mut state, catalog, origin) = world();
    let id = *state.settlements.contracts.keys().next().unwrap();
    let contract = state.settlements.contracts.get_mut(&id).unwrap();
    contract.item = "wood".into();
    contract.amount = 5;
    contract.claimed_by = Some(1);
    state.players.get_mut(&1).unwrap().inventory.clear();
    state.resource_nodes.clear();
    state.living.shops.clear();
    state.plots.clear();
    let frame = LocalFrame::new(origin);
    state.plots.insert(
        900,
        LandPlot {
            id: 900,
            owner: 2,
            faction_owner: None,
            center: origin,
            radius_m: 100.,
        },
    );
    for i in 0..=MAX_GUIDE_RESOURCE_CANDIDATES {
        let position = frame
            .to_geo([
                if i == MAX_GUIDE_RESOURCE_CANDIDATES {
                    200.
                } else {
                    21. + i as f64 * 0.05
                },
                0.,
                0.,
            ])
            .unwrap();
        let node = ResourceNode {
            id: format!("candidate-{i}"),
            position,
            item: "wood".into(),
            remaining: 5,
            capacity: 5,
            regrow_seconds: 0.,
            regrow_accumulator: 0.,
            source_feature: None,
        };
        state.resource_nodes.insert(node.id.clone(), node);
    }
    let guide = state.activity_guidance(1, &catalog, GuideTrack::Supply, Some(id));
    assert!(matches!(
        guide.action,
        Some(GuideAction::Open(GuideSurface::Nature))
    ));
    assert!(guide.detail.contains("nearest 128"));
    // Moving changes the shortlist; the distant public source can now be found.
    state
        .set_player_position(1, frame.to_geo([150., 0., 0.]).unwrap())
        .unwrap();
    let guide = state.activity_guidance(1, &catalog, GuideTrack::Supply, Some(id));
    let Some(GuideAction::Travel { position, .. }) = guide.action else {
        panic!("{guide:?}")
    };
    assert_eq!(
        position,
        state.resource_nodes[&format!("candidate-{MAX_GUIDE_RESOURCE_CANDIDATES}")].position
    );
}

#[test]
fn resumed_driving_guidance_targets_the_actual_owned_assignment() {
    let (mut state, catalog, _) = world();
    state.ensure_transport_demo(&catalog).unwrap();
    let vehicle_id = state
        .transport
        .vehicles
        .values()
        .find(|v| v.owner == 1)
        .unwrap()
        .unit_id;
    let job_id = state
        .transport
        .jobs
        .values()
        .find(|j| j.item.as_deref() == Some("wood"))
        .unwrap()
        .id;
    let vehicle_position = state.living.units[&vehicle_id].position;
    state.set_player_position(1, vehicle_position).unwrap();
    state
        .apply(
            1,
            Command::Transport {
                action: TransportCommand::EnterVehicle { vehicle_id },
            },
            &catalog,
        )
        .unwrap();
    state
        .apply(
            1,
            Command::Transport {
                action: TransportCommand::AcceptJob { job_id, vehicle_id },
            },
            &catalog,
        )
        .unwrap();
    let guide = state.activity_guidance(1, &catalog, GuideTrack::Suggested, None);
    assert!(
        matches!(guide.action,Some(GuideAction::OpenDriving {job_id:job,vehicle_id:vehicle}) if job==job_id && vehicle==vehicle_id)
    );
    let bytes = serde_json::to_vec(&state).unwrap();
    let restored: SandboxState = serde_json::from_slice(&bytes).unwrap();
    let resumed = restored.activity_guidance(1, &catalog, GuideTrack::Suggested, None);
    assert!(
        guide
            .action
            .as_ref()
            .unwrap()
            .same_intent(resumed.action.as_ref().unwrap())
    );
    assert!(
        !guide
            .action
            .as_ref()
            .unwrap()
            .same_intent(&GuideAction::OpenDriving {
                job_id: job_id + 1,
                vehicle_id
            })
    );
    assert!(
        !guide
            .action
            .as_ref()
            .unwrap()
            .same_intent(&GuideAction::OpenDriving {
                job_id,
                vehicle_id: vehicle_id + 1
            })
    );
}
