use std::collections::BTreeMap;
use terra_sandbox::*;

const COURIER: u64 = 1_000_040;

fn goods(state: &SandboxState) -> BTreeMap<String, u64> {
    let mut total = BTreeMap::new();
    for inventory in state
        .living
        .producers
        .values()
        .map(|site| &site.stock)
        .chain(state.living.shops.values().map(|shop| &shop.stock))
        .chain(state.living.units.values().map(|unit| &unit.supplies))
        .chain(state.players.values().map(|player| &player.inventory))
    {
        for (item, amount) in inventory {
            *total.entry(item.clone()).or_insert(0) += u64::from(*amount);
        }
    }
    total
}

fn money(state: &SandboxState) -> u64 {
    state.living.accounts.values().map(|a| a.money).sum::<u64>()
        + state.living.shops.values().map(|s| s.money).sum::<u64>()
        + state
            .living
            .units
            .values()
            .map(|u| u.pocket_money)
            .sum::<u64>()
}

fn replay(bytes: &[u8], label: &str) {
    let catalog = Catalog::builtin();
    let mut state: SandboxState = serde_json::from_slice(bytes).unwrap();
    state.validate_with_catalog(&catalog).unwrap();
    let initial = state.clone();
    let initial_goods = goods(&state);
    let initial_money = money(&state);
    let original_delivery = state
        .living
        .deliveries
        .iter()
        .find(|d| d.unit_id == COURIER)
        .unwrap()
        .clone();
    let source_stock = state.living.producers[&original_delivery.from_site]
        .stock
        .clone();
    let mut return_step = None;
    let mut checkpoint = None;
    for step in 1..=240 {
        let previous = state.living.units[&COURIER].position;
        state.advance_living(0.5, 0., &catalog).unwrap();
        assert_eq!(
            goods(&state),
            initial_goods,
            "{label}: goods at step {step}"
        );
        assert_eq!(
            money(&state),
            initial_money,
            "{label}: money at step {step}"
        );
        assert!(
            previous.surface_distance_m(state.living.units[&COURIER].position) <= 3.01,
            "{label}: the courier must travel within its 6 m/s handling budget"
        );
        if return_step.is_none()
            && !state
                .living
                .deliveries
                .iter()
                .any(|d| d.id == original_delivery.id)
        {
            return_step = Some(step);
            assert_eq!(
                state.living.metrics.deliveries_returned,
                initial.living.metrics.deliveries_returned + 1
            );
            assert_eq!(
                state.living.metrics.deliveries_completed,
                initial.living.metrics.deliveries_completed
            );
            let courier = &state.living.units[&COURIER];
            let source = &state.living.producers[&original_delivery.from_site];
            assert!(
                courier
                    .position
                    .surface_distance_m(source.position)
                    .hypot(courier.position.altitude_m - source.position.altitude_m)
                    <= 2.5,
                "{label}: returning cargo requires actual three-dimensional arrival"
            );
            assert!(
                courier.supplies.is_empty(),
                "{label}: returned cargo leaves the courier once"
            );
            let mut expected = source_stock.clone();
            *expected.entry(original_delivery.item.clone()).or_default() +=
                original_delivery.amount;
            assert_eq!(
                source.stock, expected,
                "{label}: source receives exactly its reserved load"
            );
        }
        if step == 120 {
            checkpoint = Some(serde_json::to_vec(&state).unwrap());
        }
    }
    state.validate_with_catalog(&catalog).unwrap();
    assert!(
        return_step.is_some(),
        "{label}: the original load must return within the unchanged 120-second window"
    );
    let mut resumed: SandboxState = serde_json::from_slice(&checkpoint.unwrap()).unwrap();
    for _ in 0..120 {
        resumed.advance_living(0.5, 0., &catalog).unwrap();
    }
    assert_eq!(
        serde_json::to_value(&resumed).unwrap(),
        serde_json::to_value(&state).unwrap()
    );
    let returned_after = return_step.unwrap() as f64 * 0.5;
    let total_goods: u64 = initial_goods.values().sum();
    assert_eq!(total_goods, 770);
    assert_eq!(initial_money, 150_000);
    println!(
        "TRAFFIC_RECOVERY_REPORT {}",
        serde_json::json!({
            "fixture": label,
            "replay_window_seconds": 120,
            "initial_physical_seconds": initial.living.traffic.elapsed_seconds,
            "return_physical_seconds": returned_after,
            "return_elapsed_physical_seconds": initial.living.traffic.elapsed_seconds + returned_after,
            "original_delivery": {
                "id": original_delivery.id,
                "item": original_delivery.item,
                "amount": original_delivery.amount,
            },
            "goods_by_item": initial_goods,
            "goods_total": total_goods,
            "money_total_minor_credits": initial_money,
            "every_step_conservation_exact": true,
            "source_return_transfer_exact": true,
            "save_resume_exact": true,
        })
    );
}

#[test]
fn historical_rear_trap_returns_its_exact_load_within_original_replay_window() {
    replay(
        include_bytes!("fixtures/traffic-rear-trap-v0.10.json"),
        "rear-trap",
    );
}

#[test]
fn historical_occupied_exit_returns_its_exact_load_within_original_replay_window() {
    replay(
        include_bytes!("fixtures/traffic-exit-held-v0.10.json"),
        "occupied-exit",
    );
}

#[test]
fn saved_loaded_queue_waits_without_fabricating_arrival_when_source_support_is_unavailable() {
    let catalog = Catalog::builtin();
    let mut state: SandboxState =
        serde_json::from_slice(include_bytes!("fixtures/traffic-exit-held-v0.10.json")).unwrap();
    let before = state.clone();
    let initial_goods = goods(&state);
    let initial_money = money(&state);
    for _ in 0..240 {
        state
            .advance_living_with_environment(0.5, 0., &catalog, |_, _, _| None)
            .unwrap();
        assert_eq!(goods(&state), initial_goods);
        assert_eq!(money(&state), initial_money);
        assert_eq!(
            state.living.units[&COURIER].position,
            before.living.units[&COURIER].position
        );
        assert_eq!(
            state.living.units[&COURIER].supplies,
            before.living.units[&COURIER].supplies
        );
    }
    assert_eq!(
        state.living.deliveries.len(),
        before.living.deliveries.len()
    );
    assert_eq!(
        state.living.deliveries[0].id,
        before.living.deliveries[0].id
    );
    assert_eq!(state.living.deliveries[0].phase, DeliveryPhase::Returning);
    assert_eq!(
        state.living.metrics.deliveries_returned,
        before.living.metrics.deliveries_returned
    );
    assert_eq!(
        state.living.metrics.deliveries_completed,
        before.living.metrics.deliveries_completed
    );
    state.validate_with_catalog(&catalog).unwrap();
}
