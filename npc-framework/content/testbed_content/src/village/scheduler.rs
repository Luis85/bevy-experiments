//! Trusted island outcomes for host-side AI continuation. Never copies game text.
use npc_framework::agent_io::AgentEvent;
use npc_framework::agent_io::scheduler::SchedulerEventSummary;
use npc_framework::prelude::*;

/// Classifies only successful island outcomes involving the assigned actor.
/// Unknown effects, public diaries and other actors' work remain outside this
/// channel. The closed return type cannot carry names, prices, pages or tokens.
pub(super) fn classify(event: &AgentEvent, actor: SimId) -> Option<SchedulerEventSummary> {
    let JournalKind::Effect {
        effect,
        applied: true,
    } = &event.event
    else {
        return None;
    };
    let kind = effect.get("type")?.as_str()?;
    if kind == "VillageTrade" {
        let participant = |key| {
            effect
                .get(key)?
                .as_u64()
                .and_then(|id| u32::try_from(id).ok())
                .filter(|id| *id != 0)
                .map(SimId)
        };
        let buyer = participant("buyer")?;
        let seller = participant("seller")?;
        // Each participant has one receipt. The original trade is the buyer's;
        // VillageSaleCompleted is the seller's, so shared rings cannot double
        // count a sale when both participants have external controllers.
        return (event.unit == Some(buyer) && actor == buyer && seller != buyer)
            .then_some(SchedulerEventSummary::TradeCompleted);
    }
    if event.unit != Some(actor) {
        return None;
    }
    match kind {
        "VillageGathered"
        | "VillageHunted"
        | "VillageRestCompleted"
        | "VillageCottageBuilt"
        | "VillageCropReady"
        | "VillageCropHarvested" => Some(SchedulerEventSummary::WorkCompleted),
        "VillageHuntFailed" => Some(SchedulerEventSummary::ActionFailed),
        "VillageCottageFailed" => Some(SchedulerEventSummary::ActionFailed),
        "VillageHuntStopped" => Some(SchedulerEventSummary::ActionCancelled),
        "VillageCottageCancelled" => Some(SchedulerEventSummary::ActionCancelled),
        "VillageClothingWornOut" => Some(SchedulerEventSummary::InventoryChanged),
        "VillageGoodsOffered"
        | "VillageGoodsWithdrawn"
        | "VillagePlotClaimed"
        | "VillageCropPlanted"
        | "VillageCropWatered" => Some(SchedulerEventSummary::ActionCompleted),
        "VillageConsumed" => Some(SchedulerEventSummary::NeedsRestored),
        "VillageBedRented" | "VillageRested" => Some(SchedulerEventSummary::RestStarted),
        "VillageSaleCompleted" => Some(SchedulerEventSummary::TradeCompleted),
        "VillageSpeechHeard" => Some(SchedulerEventSummary::IncomingSpeech),
        _ => None,
    }
}
