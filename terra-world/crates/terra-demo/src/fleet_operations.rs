//! Retained operands for ordinary Fleet operations. Live timers and payroll are
//! intentionally absent; authority rechecks eligibility immediately afterward.
use super::*;
use crate::reviewed_action::{NativeContext, next_token};
use terra_core::GeoPosition;

const ACTIONS: [FleetAction; 13] = [
    FleetAction::Deposit,
    FleetAction::Withdraw,
    FleetAction::Assign,
    FleetAction::Run,
    FleetAction::Pause,
    FleetAction::Release,
    FleetAction::Hold,
    FleetAction::Handover,
    FleetAction::OpenDepot,
    FleetAction::DepositGoods,
    FleetAction::WithdrawGoods,
    FleetAction::AppendStop,
    FleetAction::SaveRoute,
];

#[derive(Clone)]
pub(super) struct OperationReview {
    action: FleetAction,
    token: u64,
    context: OperationContext,
}

#[derive(Clone, PartialEq)]
struct OperationContext {
    actor: NativeContext,
    tab: FleetTab,
    company_name: String,
    selections: [Option<u64>; 6],
    target: OperationTarget,
}

#[derive(Clone, PartialEq)]
struct DriverIdentity {
    id: u64,
    name: String,
    vehicle_id: Option<u64>,
    route_id: Option<u64>,
    next_stop: usize,
    running: bool,
    status: FleetStatus,
    trip: Option<FleetTrip>,
    assigned_vehicle: Option<VehicleIdentity>,
    assigned_route: Option<CompanyRoute>,
}

impl DriverIdentity {
    fn capture(s: &SandboxRuntime, driver: &CompanyDriver) -> Self {
        Self {
            id: driver.id,
            name: driver.name.clone(),
            vehicle_id: driver.vehicle_id,
            route_id: driver.route_id,
            next_stop: driver.next_stop,
            running: driver.running,
            status: driver.status,
            trip: driver.trip.clone(),
            assigned_vehicle: driver
                .vehicle_id
                .and_then(|id| s.state.transport.vehicles.get(&id))
                .map(VehicleIdentity::from),
            assigned_route: company(s)
                .and_then(|company| {
                    company
                        .routes
                        .iter()
                        .find(|route| Some(route.id) == driver.route_id)
                })
                .cloned(),
        }
    }
}

#[derive(Clone, PartialEq)]
struct VehicleIdentity {
    id: u64,
    owner: u64,
    model: VehicleModel,
    cargo: Inventory,
    trailer: Option<u64>,
    // Lease identity and retained money are stable; the live prepaid countdown
    // changes each frame and is checked through current operation eligibility.
    rental_terms: Option<(u64, u64)>,
}

impl From<&VehicleState> for VehicleIdentity {
    fn from(vehicle: &VehicleState) -> Self {
        Self {
            id: vehicle.unit_id,
            owner: vehicle.owner,
            model: vehicle.model,
            cargo: vehicle.cargo.clone(),
            trailer: vehicle.trailer,
            rental_terms: vehicle
                .rental
                .as_ref()
                .map(|rental| (rental.facility_id, rental.bond_escrow)),
        }
    }
}

#[derive(Clone, PartialEq)]
struct FacilityIdentity {
    id: u64,
    name: String,
    kind: TransportFacilityKind,
    position: GeoPosition,
    linked_site: Option<u64>,
}

impl From<&TransportFacility> for FacilityIdentity {
    fn from(facility: &TransportFacility) -> Self {
        Self {
            id: facility.id,
            name: facility.name.clone(),
            kind: facility.kind,
            position: facility.position,
            linked_site: facility.linked_site,
        }
    }
}

#[derive(Clone, PartialEq)]
enum OperationTarget {
    Cash(u64),
    Assign(DriverIdentity, VehicleIdentity, CompanyRoute),
    Driver(DriverIdentity),
    Handover(DriverIdentity, DriverIdentity, VehicleIdentity),
    Depot(FacilityIdentity),
    Stock(FacilityIdentity, String, u32),
    Stop(FacilityIdentity, Vec<u64>, FleetService),
    Route(String, Vec<FacilityIdentity>, FleetService),
}

pub(crate) fn requires_operation_review(action: FleetAction) -> bool {
    ACTIONS.contains(&action)
}

fn visible(action: FleetAction, ui: &UiState) -> bool {
    if !ui.context_panel || ui.panel != Panel::Fleet {
        return false;
    }
    use FleetAction as A;
    match action {
        A::Deposit | A::Withdraw => ui.fleet.tab == FleetTab::Company,
        A::Assign | A::Pause | A::Release => ui.fleet.tab == FleetTab::Drivers,
        A::Run => matches!(ui.fleet.tab, FleetTab::Drivers | FleetTab::Recovery),
        A::Hold | A::Handover => ui.fleet.tab == FleetTab::Recovery,
        A::OpenDepot | A::DepositGoods | A::WithdrawGoods => ui.fleet.tab == FleetTab::Depot,
        A::AppendStop | A::SaveRoute => ui.fleet.tab == FleetTab::Routes,
        _ => false,
    }
}

fn capture(action: FleetAction, s: &SandboxRuntime, ui: &UiState) -> Option<OperationContext> {
    if !visible(action, ui) {
        return None;
    }
    let company = company(s)?;
    if company.owner != s.player_id {
        return None;
    }
    use FleetAction as A;
    let target = match action {
        A::Deposit | A::Withdraw => OperationTarget::Cash(ui.fleet.transfer),
        A::Assign => OperationTarget::Assign(
            DriverIdentity::capture(s, driver(s, ui)?),
            vehicle(s, ui)?.into(),
            route(s, ui)?.clone(),
        ),
        A::Run | A::Pause | A::Release | A::Hold => {
            OperationTarget::Driver(DriverIdentity::capture(s, driver(s, ui)?))
        }
        A::Handover => OperationTarget::Handover(
            DriverIdentity::capture(s, driver(s, ui)?),
            DriverIdentity::capture(s, relief(s, ui)?),
            vehicle(s, ui)?.into(),
        ),
        A::OpenDepot => OperationTarget::Depot(depot_facility(s, ui)?.into()),
        A::DepositGoods | A::WithdrawGoods => OperationTarget::Stock(
            depot_facility(s, ui)?.into(),
            interface::TRADE_ITEMS[ui.fleet.item % interface::TRADE_ITEMS.len()]
                .0
                .into(),
            ui.fleet.quantity,
        ),
        A::AppendStop => OperationTarget::Stop(
            stop(s, ui)?.into(),
            ui.fleet.draft_stops.clone(),
            service(ui),
        ),
        A::SaveRoute => OperationTarget::Route(
            ui.fleet.route_name.clone(),
            ui.fleet
                .draft_stops
                .iter()
                .map(|id| {
                    s.state
                        .transport
                        .facilities
                        .get(id)
                        .map(FacilityIdentity::from)
                })
                .collect::<Option<Vec<_>>>()?,
            service(ui),
        ),
        _ => return None,
    };
    Some(OperationContext {
        actor: NativeContext::capture(s)?,
        tab: ui.fleet.tab,
        company_name: company.name.clone(),
        selections: [
            ui.fleet.driver,
            ui.fleet.vehicle,
            ui.fleet.route,
            ui.fleet.stop,
            ui.fleet.relief,
            ui.fleet.depot,
        ],
        target,
    })
}

/// Called at the label cadence, so input never silently reviews a new operand.
pub(crate) fn refresh_displayed_operations(s: &SandboxRuntime, ui: &mut UiState) {
    let mut next = Vec::new();
    for action in ACTIONS {
        let Some(context) = capture(action, s, ui) else {
            continue;
        };
        if let Some(previous) = ui
            .fleet
            .displayed_operations
            .iter()
            .find(|shown| shown.action == action && shown.context == context)
        {
            next.push(previous.clone());
        } else if let Some(token) = next_token() {
            next.push(OperationReview {
                action,
                token,
                context,
            });
        }
    }
    ui.fleet.displayed_operations = next;
}

pub(crate) fn bind_operation(ui: &UiState, action: FleetAction) -> Option<Action> {
    if requires_operation_review(action) {
        ui.fleet
            .displayed_operations
            .iter()
            .find(|shown| shown.action == action)
            .map(|shown| Action::FleetReviewed {
                action,
                token: shown.token,
            })
    } else {
        Some(Action::Fleet(action))
    }
}

pub(crate) fn act_reviewed_operation(
    action: FleetAction,
    token: u64,
    ui: &mut UiState,
    s: &mut SandboxRuntime,
) -> Option<Command> {
    let current = capture(action, s, ui);
    let matched = ui.fleet.displayed_operations.iter().any(|shown| {
        shown.action == action && shown.token == token && current.as_ref() == Some(&shown.context)
    });
    if !matched || unavailable(action, s, ui) {
        s.last_message = "The displayed Fleet operation is no longer ready, or its actor, selection, assignment, load or amount changed. Review the named operation and requirements again.".into();
        return None;
    }
    // One displayed operation cannot execute twice from an already queued token.
    ui.fleet
        .displayed_operations
        .retain(|shown| shown.token != token);
    act(action, ui, s)
}
