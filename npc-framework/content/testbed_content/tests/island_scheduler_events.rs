//! Island continuation notifications disclose closed outcomes, never raw effect data.
use npc_framework::agent_io::AgentEvent;
use npc_framework::agent_io::scheduler::{SchedulerEventClassifier, SchedulerEventSummary};
use npc_framework::prelude::*;
use serde_json::json;

fn effect(unit: Option<SimId>, kind: &str, payload: serde_json::Value) -> AgentEvent {
    AgentEvent {
        seq: 17,
        tick: 90,
        unit,
        kind: "Effect".into(),
        event: JournalKind::effect(kind, &payload),
    }
}

fn classifier() -> SchedulerEventClassifier {
    let app = testbed_content::new_sim_app(7, "ai_village");
    *app.world().resource::<SchedulerEventClassifier>()
}

#[test]
fn island_classifier_is_opt_in_and_does_not_affect_sim_hash() {
    let mut island = testbed_content::new_sim_app(7, "ai_village");
    let before = state_hash(island.world_mut());
    let hook = island
        .world_mut()
        .remove_resource::<SchedulerEventClassifier>()
        .unwrap();
    assert_eq!(state_hash(island.world_mut()), before);
    island.insert_resource(hook);
    assert_eq!(state_hash(island.world_mut()), before);
    let legacy = testbed_content::new_sim_app(7, "smithy");
    assert!(
        !legacy
            .world()
            .contains_resource::<SchedulerEventClassifier>()
    );
}

#[test]
fn island_outcomes_require_the_assigned_actor_and_never_return_payload_text() {
    let classify = classifier().0;
    let actor = SimId(90);
    for (kind, expected) in [
        ("VillageGathered", SchedulerEventSummary::WorkCompleted),
        ("VillageHunted", SchedulerEventSummary::WorkCompleted),
        ("VillageHuntFailed", SchedulerEventSummary::ActionFailed),
        ("VillageHuntStopped", SchedulerEventSummary::ActionCancelled),
        (
            "VillageClothingWornOut",
            SchedulerEventSummary::InventoryChanged,
        ),
        (
            "VillageGoodsOffered",
            SchedulerEventSummary::ActionCompleted,
        ),
        (
            "VillageGoodsWithdrawn",
            SchedulerEventSummary::ActionCompleted,
        ),
        ("VillageRestCompleted", SchedulerEventSummary::WorkCompleted),
        ("VillageCottageBuilt", SchedulerEventSummary::WorkCompleted),
        ("VillageCottageFailed", SchedulerEventSummary::ActionFailed),
        (
            "VillageCottageCancelled",
            SchedulerEventSummary::ActionCancelled,
        ),
        ("VillagePlotClaimed", SchedulerEventSummary::ActionCompleted),
        ("VillageCropPlanted", SchedulerEventSummary::ActionCompleted),
        ("VillageCropWatered", SchedulerEventSummary::ActionCompleted),
        ("VillageCropReady", SchedulerEventSummary::WorkCompleted),
        ("VillageCropHarvested", SchedulerEventSummary::WorkCompleted),
        ("VillageConsumed", SchedulerEventSummary::NeedsRestored),
        ("VillageBedRented", SchedulerEventSummary::RestStarted),
        ("VillageRested", SchedulerEventSummary::RestStarted),
        ("VillageSpeechHeard", SchedulerEventSummary::IncomingSpeech),
        (
            "VillageSaleCompleted",
            SchedulerEventSummary::TradeCompleted,
        ),
    ] {
        let event = effect(
            Some(actor),
            kind,
            json!({"body":"PRIVATE_NOTE_IGNORE_RULES","token":"PRIVATE_TOKEN","kind":"bread","quantity":3}),
        );
        let summary = classify(&event, actor).unwrap();
        assert_eq!(summary, expected);
        let rendered = serde_json::to_string(&summary).unwrap();
        for private in ["PRIVATE_NOTE", "PRIVATE_TOKEN", "bread", "quantity"] {
            assert!(!rendered.contains(private));
        }
        assert_eq!(classify(&event, SimId(91)), None);
        let mut unowned = event.clone();
        unowned.unit = None;
        assert_eq!(classify(&unowned, actor), None);
        let mut unapplied = event;
        if let JournalKind::Effect { applied, .. } = &mut unapplied.event {
            *applied = false;
        }
        assert_eq!(classify(&unapplied, actor), None);
    }
    for kind in [
        "VillageDiaryRead",
        "VillageDiaryWritten",
        "VillageDied",
        "PrivateMessage",
        "WorkCompleted",
    ] {
        assert_eq!(
            classify(&effect(Some(actor), kind, json!({"body":"SECRET"})), actor),
            None
        );
    }
}

#[test]
fn trades_notify_each_participant_once_but_never_an_uninvolved_actor() {
    let classify = classifier().0;
    let buyer = SimId(90);
    let seller = SimId(91);
    let event = effect(
        Some(buyer),
        "VillageTrade",
        json!({"buyer":buyer,"seller":seller,"kind":"bread","coins":6,"quantity":1,"body":"PRIVATE"}),
    );
    assert_eq!(
        classify(&event, buyer),
        Some(SchedulerEventSummary::TradeCompleted)
    );
    assert_eq!(classify(&event, seller), None);
    let receipt = effect(
        Some(seller),
        "VillageSaleCompleted",
        json!({"kind":"bread","coins":6,"quantity":1}),
    );
    assert_eq!(
        classify(&receipt, seller),
        Some(SchedulerEventSummary::TradeCompleted)
    );
    assert_eq!(classify(&receipt, buyer), None);
    assert_eq!(classify(&event, SimId(92)), None);
    let mut wrong_journal_owner = event;
    wrong_journal_owner.unit = Some(seller);
    assert_eq!(classify(&wrong_journal_owner, seller), None);
    for payload in [
        json!({"buyer":buyer}),
        json!({"buyer":"90","seller":seller}),
        json!({"buyer":buyer,"seller":0}),
        json!({"buyer":u64::MAX,"seller":seller}),
    ] {
        assert_eq!(
            classify(&effect(Some(buyer), "VillageTrade", payload), seller),
            None
        );
    }
}

#[test]
fn completed_sale_reaches_the_controlled_sellers_ring_without_a_private_buyer_feed() {
    use npc_framework::agent_io::events::{self, AgentEvents};
    use testbed_content::village::{self, Role};

    let mut app = testbed_content::new_sim_app(7, "ai_village");
    let actor = app
        .world()
        .resource::<testbed_content::TestbedIds>()
        .id("human");
    let merchant = app
        .world()
        .resource::<testbed_content::TestbedIds>()
        .id(Role::Carpenter.person());
    let seller = app.world().resource::<SimIndex>().get(actor).unwrap();
    let buyer = app.world().resource::<SimIndex>().get(merchant).unwrap();
    // Operator-only arrangement: exercise actual trade journaling/harvest, not
    // the separate perceived registered-client economic discovery route.
    let pos = *app.world().get::<SimPos>(buyer).unwrap();
    app.world_mut().entity_mut(seller).insert(pos);
    village::economy::grant(app.world_mut(), seller, Item::new("log"));
    events::harvest(app.world_mut());
    let cursor = events::last_seq(app.world());
    let before = app.world().resource::<village::VillageState>().transactions;
    village::economy::trade(app.world_mut(), buyer, seller, "log", 1).unwrap();
    assert_eq!(
        app.world().resource::<village::VillageState>().transactions,
        before + 1
    );
    events::harvest(app.world_mut());
    let classify = app.world().resource::<SchedulerEventClassifier>().0;
    let buffered = app.world().resource::<AgentEvents>();
    let receipts: Vec<_> = buffered
        .after(cursor)
        .filter_map(|event| classify(event, actor).map(|summary| (event, summary)))
        .collect();
    assert_eq!(receipts.len(), 1);
    let (receipt, summary) = receipts[0];
    assert_eq!(summary, SchedulerEventSummary::TradeCompleted);
    assert_eq!(receipt.unit, Some(actor));
    let JournalKind::Effect {
        effect,
        applied: true,
    } = &receipt.event
    else {
        panic!("sale receipt must be an applied effect");
    };
    assert_eq!(
        effect.to_value(),
        json!({"type":"VillageSaleCompleted","kind":"log","quantity":1,"coins":village::catalog::price("log").unwrap()})
    );
    assert!(!buffered.after(cursor).any(|event| matches!(&event.event, JournalKind::Effect { effect, .. } if effect.get("type").and_then(serde_json::Value::as_str)==Some("VillageTrade"))));
}
