//! Connected company management: every financial and dispatch action goes
//! through the same domain command boundary as a remote client.
use crate::interface::{self, Action, InputMode, Label, Panel, UiState};
use bevy::prelude::*;
use terra_bevy::rules::Command;
use terra_bevy::{SandboxRuntime, rules::*};

#[path = "fleet_operations.rs"]
mod operations;
pub(crate) use operations::{
    act_reviewed_operation, bind_operation, refresh_displayed_operations, requires_operation_review,
};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum FleetTab {
    #[default]
    Company,
    Finances,
    Drivers,
    Routes,
    Recovery,
    Depot,
}
impl FleetTab {
    fn title(self) -> &'static str {
        match self {
            Self::Company => "Company",
            Self::Finances => "Finances",
            Self::Drivers => "Drivers",
            Self::Routes => "Routes",
            Self::Recovery => "Recovery",
            Self::Depot => "Depot",
        }
    }
}
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum FleetDraftKind {
    #[default]
    Passengers,
    PublicFreight,
    WarehouseSale,
    WarehouseTransfer,
}
pub struct FleetUi {
    pub tab: FleetTab,
    pub navigation_open: bool,
    pub driver: Option<u64>,
    pub vehicle: Option<u64>,
    pub route: Option<u64>,
    pub stop: Option<u64>,
    pub route_name: String,
    pub draft_stops: Vec<u64>,
    pub service_kind: FleetDraftKind,
    pub market_shop: Option<u64>,
    pub item: usize,
    pub quantity: u32,
    pub transfer: u64,
    confirmation: Option<FleetConfirmation>,
    pub relief: Option<u64>,
    pub depot: Option<u64>,
    displayed_quotes: DisplayedFleetQuotes,
    displayed_operations: Vec<operations::OperationReview>,
}

#[derive(Default)]
struct DisplayedFleetQuotes {
    player_id: u64,
    restore_revision: u64,
    depot_buy: Option<CompanyDepotTradeQuote>,
    depot_sell: Option<CompanyDepotTradeQuote>,
    roadside: Option<CompanyRoadsideQuote>,
}

#[derive(Clone, PartialEq)]
struct FleetConfirmation {
    player_id: u64,
    restore_revision: u64,
    panel: Panel,
    tab: FleetTab,
    target: FleetConfirmationTarget,
}

#[derive(Clone, PartialEq)]
enum FleetConfirmationTarget {
    FundServices(u64),
    Dismiss {
        driver_id: u64,
        name: String,
        vehicle_id: Option<u64>,
    },
    Return {
        driver_id: u64,
        vehicle_id: Option<u64>,
        trip: FleetTrip,
    },
    CloseDepot(u64),
    DeleteRoute(CompanyRoute),
}

impl FleetConfirmationTarget {
    fn matches_action(&self, action: FleetAction) -> bool {
        matches!(
            (self, action),
            (Self::FundServices(_), FleetAction::FundServices)
                | (Self::Dismiss { .. }, FleetAction::Dismiss)
                | (Self::Return { .. }, FleetAction::Return)
                | (Self::CloseDepot(_), FleetAction::CloseDepot)
                | (Self::DeleteRoute(_), FleetAction::DeleteRoute)
        )
    }

    fn command(self) -> CompanyCommand {
        match self {
            Self::FundServices(amount) => CompanyCommand::FundPublicServices { amount },
            Self::Dismiss { driver_id, .. } => CompanyCommand::DismissDriver { driver_id },
            Self::Return { driver_id, .. } => CompanyCommand::ReturnLoad { driver_id },
            Self::CloseDepot(facility_id) => CompanyCommand::CloseDepot { facility_id },
            Self::DeleteRoute(route) => CompanyCommand::DeleteRoute { route_id: route.id },
        }
    }
}

fn capture_confirmation(
    action: FleetAction,
    s: &SandboxRuntime,
    ui: &UiState,
) -> Option<FleetConfirmation> {
    company(s)?;
    let target = match action {
        FleetAction::FundServices => FleetConfirmationTarget::FundServices(ui.fleet.transfer),
        FleetAction::Dismiss => {
            let driver = driver(s, ui)?;
            FleetConfirmationTarget::Dismiss {
                driver_id: driver.id,
                name: driver.name.clone(),
                vehicle_id: driver.vehicle_id,
            }
        }
        FleetAction::Return => {
            let driver = driver(s, ui)?;
            FleetConfirmationTarget::Return {
                driver_id: driver.id,
                vehicle_id: driver.vehicle_id,
                trip: driver.trip.as_ref().filter(|trip| !trip.returning)?.clone(),
            }
        }
        FleetAction::CloseDepot => {
            FleetConfirmationTarget::CloseDepot(owned_depot(s, ui)?.facility_id)
        }
        FleetAction::DeleteRoute => FleetConfirmationTarget::DeleteRoute(route(s, ui)?.clone()),
        _ => return None,
    };
    Some(FleetConfirmation {
        player_id: s.player_id,
        restore_revision: s.restore_revision,
        panel: ui.panel,
        tab: ui.fleet.tab,
        target,
    })
}

fn confirmation_current(action: FleetAction, s: &SandboxRuntime, ui: &UiState) -> bool {
    ui.fleet
        .confirmation
        .as_ref()
        .is_some_and(|shown| capture_confirmation(action, s, ui).as_ref() == Some(shown))
}

/// A second click submits only the exact player, restored world and target
/// reviewed by the first. Chooser fallbacks and a new trip cannot inherit an
/// earlier confirmation, even if no intervening Fleet button was clicked.
fn confirmed_action(
    action: FleetAction,
    ui: &mut UiState,
    s: &mut SandboxRuntime,
) -> Option<CompanyCommand> {
    let Some(current) = capture_confirmation(action, s, ui) else {
        ui.fleet.confirmation = None;
        s.last_message =
            "This fleet target is no longer available. Choose it again before confirming.".into();
        return None;
    };
    let previous = ui.fleet.confirmation.take();
    if previous.as_ref() == Some(&current) {
        return Some(current.target.command());
    }
    ui.fleet.confirmation = Some(current);
    s.last_message = if previous.is_some() {
        "Fleet selection, load, amount or restored world changed. Review the newly named action and confirm again."
    } else {
        "Review the named fleet action, then choose its confirmation. Another Fleet action cancels this review."
    }.into();
    None
}

/// Refresh only with the rendered labels, never while accepting an action.
pub fn refresh_displayed_quotes(s: &SandboxRuntime, ui: &mut UiState) {
    let visible = ui.context_panel && ui.panel == Panel::Fleet;
    ui.fleet.displayed_quotes = DisplayedFleetQuotes {
        player_id: s.player_id,
        restore_revision: s.restore_revision,
        depot_buy: (visible && ui.fleet.tab == FleetTab::Depot)
            .then(|| depot_market_quote(s, ui, true).ok())
            .flatten(),
        depot_sell: (visible && ui.fleet.tab == FleetTab::Depot)
            .then(|| depot_market_quote(s, ui, false).ok())
            .flatten(),
        roadside: (visible && ui.fleet.tab == FleetTab::Recovery)
            .then(|| roadside_offer(s, ui).ok())
            .flatten(),
    };
}

fn reviewed_depot_quote(
    s: &SandboxRuntime,
    ui: &UiState,
    buy: bool,
) -> Result<CompanyDepotTradeQuote, String> {
    let current = depot_market_quote(s, ui, buy)?;
    let shown = &ui.fleet.displayed_quotes;
    let expected = if buy {
        &shown.depot_buy
    } else {
        &shown.depot_sell
    };
    if !ui.context_panel
        || ui.panel != Panel::Fleet
        || ui.fleet.tab != FleetTab::Depot
        || shown.player_id != s.player_id
        || shown.restore_revision != s.restore_revision
        || expected.as_ref() != Some(&current)
    {
        return Err(
            "Depot price or selection changed. Review the displayed quote and try again.".into(),
        );
    }
    Ok(current)
}

fn reviewed_roadside_quote(
    s: &SandboxRuntime,
    ui: &UiState,
) -> Result<CompanyRoadsideQuote, String> {
    let current = roadside_offer(s, ui)?;
    let shown = &ui.fleet.displayed_quotes;
    if !ui.context_panel
        || ui.panel != Panel::Fleet
        || ui.fleet.tab != FleetTab::Recovery
        || shown.player_id != s.player_id
        || shown.restore_revision != s.restore_revision
        || shown.roadside != Some(current)
    {
        return Err(
            "Roadside service quote changed. Review the displayed service and try again.".into(),
        );
    }
    Ok(current)
}
impl Default for FleetUi {
    fn default() -> Self {
        Self {
            tab: FleetTab::Company,
            navigation_open: false,
            driver: None,
            vehicle: None,
            route: None,
            stop: None,
            route_name: "Town service".into(),
            draft_stops: Vec::new(),
            service_kind: FleetDraftKind::Passengers,
            market_shop: None,
            item: 0,
            quantity: 1,
            transfer: 5_000,
            confirmation: None,
            relief: None,
            depot: None,
            displayed_quotes: DisplayedFleetQuotes::default(),
            displayed_operations: Vec::new(),
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FleetAction {
    Tab(FleetTab),
    ToggleNavigation,
    Register,
    TransferAmount,
    Deposit,
    Withdraw,
    FundServices,
    Hire,
    NextDriver,
    NextVehicle,
    NextRoute,
    Assign,
    Run,
    Pause,
    Release,
    Hold,
    Return,
    Dismiss,
    NextRelief,
    Handover,
    NextDepot,
    OpenDepot,
    CloseDepot,
    DepotItem,
    DepotQuantity,
    DepositGoods,
    WithdrawGoods,
    NextMarket,
    BuyDepotGoods,
    SellDepotGoods,
    SuggestDepotSale,
    SuggestDepotTransfer,
    RoadsideService,
    EditRouteName,
    Service,
    Item,
    Quantity,
    NextStop,
    AppendStop,
    UndoStop,
    ClearStops,
    SaveRoute,
    SuggestRoute,
    DeleteRoute,
}
#[derive(Clone, Copy)]
pub enum FleetLabel {
    Summary,
    Navigation,
    FinanceResult,
    FinanceCash,
    FinanceFunding,
    FinanceCategories,
    FinanceScope,
    FinanceEntries,
    Registration,
    FundServices,
    Transfer,
    Driver,
    Vehicle,
    Route,
    RouteEconomics,
    Draft,
    DraftEconomics,
    Stop,
    Prerequisites,
    Roadside,
    Recovery,
    Depot,
    DepotMarket,
    DepotBuy,
    DepotSell,
    Dismiss,
    Return,
    CloseDepot,
    Delete,
}
#[derive(Component)]
struct FleetSection(FleetTab);
#[derive(Component)]
struct FleetNavigation;
#[derive(Component)]
struct FleetNavigationToggle;
#[derive(Component)]
struct FleetFinanceDetails;
pub struct FleetPlugin;
impl Plugin for FleetPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, sections.before(crate::interface::InterfaceLayout));
    }
}
fn sections(
    ui: Res<UiState>,
    sandbox: Res<SandboxRuntime>,
    settings: Res<crate::player_input::InputSettings>,
    windows: Query<&Window>,
    mut nodes: Query<
        (
            &mut Node,
            Option<&FleetSection>,
            Has<FleetNavigation>,
            Has<FleetNavigationToggle>,
            Has<FleetFinanceDetails>,
        ),
        Or<(
            With<FleetSection>,
            With<FleetNavigation>,
            With<FleetNavigationToggle>,
            With<FleetFinanceDetails>,
        )>,
    >,
) {
    let compact = windows.iter().next().is_some_and(|window| {
        interface::compact_inspector_navigation(window.height(), settings.text_scale)
    });
    for (mut node, section, navigation, toggle, finance_details) in &mut nodes {
        let visible = if let Some(section) = section {
            section.0 == ui.fleet.tab
        } else if navigation {
            !compact || ui.fleet.navigation_open
        } else if finance_details {
            company(&sandbox).is_some()
        } else {
            toggle && compact
        };
        let display = if visible {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != display {
            node.display = display;
        }
    }
}
fn section(
    p: &mut ChildSpawnerCommands,
    tab: FleetTab,
    build: impl FnOnce(&mut ChildSpawnerCommands),
) {
    p.spawn((
        FleetSection(tab),
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: px(8),
            flex_shrink: 0.,
            ..default()
        },
    ))
    .with_children(build);
}
fn line(p: &mut ChildSpawnerCommands, label: FleetLabel, prominent: bool) {
    p.spawn((
        Label::Fleet(label),
        interface::text(
            "",
            if prominent { 14. } else { 12. },
            if prominent {
                interface::TEXT
            } else {
                interface::MUTED
            },
        ),
    ));
}
fn button(p: &mut ChildSpawnerCommands, title: &str, action: FleetAction) {
    interface::button(p, title, Action::Fleet(action));
}
pub fn panels(p: &mut ChildSpawnerCommands) {
    use FleetAction as A;
    interface::page(p, Panel::Fleet, |p| {
        line(p, FleetLabel::Summary, true);
        p.spawn((
            FleetNavigationToggle,
            Node {
                width: percent(100),
                flex_shrink: 0.,
                ..default()
            },
        ))
        .with_children(|p| {
            interface::button_with_label(
                p,
                "Company · choose section",
                Action::Fleet(A::ToggleNavigation),
                Some(Label::Fleet(FleetLabel::Navigation)),
            )
        });
        p.spawn((
            FleetNavigation,
            Node {
                width: percent(100),
                column_gap: px(4),
                row_gap: px(4),
                flex_wrap: FlexWrap::Wrap,
                flex_shrink: 0.,
                ..default()
            },
        ))
        .with_children(|p| {
            for (title, tab) in [
                ("Company", FleetTab::Company),
                ("Finances", FleetTab::Finances),
                ("Drivers", FleetTab::Drivers),
                ("Routes", FleetTab::Routes),
                ("Recovery", FleetTab::Recovery),
                ("Depot", FleetTab::Depot),
            ] {
                p.spawn(interface::inspector_tab_cell())
                    .with_children(|p| button(p, title, A::Tab(tab)));
            }
        });
        section(p, FleetTab::Company, |p| {
            interface::heading(p, "Company registration");
            line(p, FleetLabel::Registration, false);
            button(p, "Name and register company · 20 cr", A::Register);
            interface::heading(p, "Company account");
            line(p, FleetLabel::Transfer, true);
            button(p, "Change transfer · 10 / 50 / 100 cr", A::TransferAmount);
            interface::pair(
                p,
                ("Deposit funds", Action::Fleet(A::Deposit)),
                ("Withdraw funds", Action::Fleet(A::Withdraw)),
            );
            button(
                p,
                "Review income, costs and funding",
                A::Tab(FleetTab::Finances),
            );
            p.spawn(interface::text("Company cash pays recruitment, stock and wages. Public fares come from the service treasury; warehouse sales are paid by the buying shop. Earned wages belong to employees; they remain recorded when a vehicle is released.", 12., interface::MUTED));
            interface::button_with_label(
                p,
                "Contribute to public service fund",
                Action::Fleet(A::FundServices),
                Some(Label::Fleet(FleetLabel::FundServices)),
            );
            p.spawn(interface::text("If the public issuer runs out of fare money, use the selected transfer amount to support shared transport services. This contribution leaves company cash permanently; every credit is conserved.",12.,interface::MUTED));
            interface::heading(p, "Your next steps");
            p.spawn(interface::text("1. Register at a garage and deposit funds.\n2. Hire a driver at a garage.\n3. Create a route between service stops.\n4. Exit your vehicle, assign it, and start work.\n\nFleet work uses physical time. Offline companies freeze. Traffic, stock, fuel and payroll can pause a route; the driver explains why.", 12., interface::TEXT));
            interface::button(
                p,
                "Find a garage",
                Action::Driving(crate::driving::DrivingAction::RouteGarage),
            );
        });
        section(p, FleetTab::Finances, |p| {
            interface::heading(p, "Company cash result");
            line(p, FleetLabel::FinanceResult, true);
            button(p, "Manage company", A::Tab(FleetTab::Company));
            p.spawn((
                FleetFinanceDetails,
                Node {
                    width: percent(100),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(8),
                    flex_shrink: 0.,
                    ..default()
                },
            ))
            .with_children(|p| {
                interface::heading(p, "Cash movement");
                line(p, FleetLabel::FinanceCash, false);
                interface::heading(p, "Owner funding and costs");
                line(p, FleetLabel::FinanceFunding, false);
                interface::heading(p, "Recorded categories");
                line(p, FleetLabel::FinanceCategories, false);
                interface::heading(p, "What this report covers");
                line(p, FleetLabel::FinanceScope, false);
                interface::heading(p, "Recent entries");
                line(p, FleetLabel::FinanceEntries, false);
            });
        });
        section(p, FleetTab::Drivers, |p| {
            interface::heading(p, "Your employees");
            line(p, FleetLabel::Driver, true);
            button(p, "Choose next employee", A::NextDriver);
            button(p, "Hire named driver · 10 company cr", A::Hire);
            interface::button_with_label(
                p,
                "Dismiss unassigned employee",
                Action::Fleet(A::Dismiss),
                Some(Label::Fleet(FleetLabel::Dismiss)),
            );
            line(p, FleetLabel::Vehicle, false);
            button(p, "Choose next owned vehicle", A::NextVehicle);
            line(p, FleetLabel::Route, false);
            button(p, "Choose next saved route", A::NextRoute);
            button(p, "Assign driver, vehicle and route", A::Assign);
            line(p, FleetLabel::Prerequisites, false);
            interface::heading(p, "Before dispatch");
            line(p, FleetLabel::RouteEconomics, false);
            interface::pair(
                p,
                ("Start / resume", Action::Fleet(A::Run)),
                ("Pause safely", Action::Fleet(A::Pause)),
            );
            button(p, "Release parked empty vehicle", A::Release);
            p.spawn(interface::text("Payroll: 1.20 cr per physical minute while dispatched, including road delays. Pause finishes a loaded delivery before parking. Release makes the empty vehicle available for your driving and jobs.", 12., interface::MUTED));
            button(
                p,
                "Recover or hand over a loaded vehicle",
                A::Tab(FleetTab::Recovery),
            );
        });
        section(p, FleetTab::Recovery, |p| {
            interface::heading(p, "Recover a working vehicle");
            line(p, FleetLabel::Driver, true);
            button(p, "Choose next employee", A::NextDriver);
            button(p, "Hold vehicle here · stop payroll", A::Hold);
            interface::button_with_label(
                p,
                "Return load to original pickup",
                Action::Fleet(A::Return),
                Some(Label::Fleet(FleetLabel::Return)),
            );
            button(p, "Resume delivery / return", A::Run);
            p.spawn(interface::text("Hold parks the vehicle with its load. Return physically takes goods or passengers back to their pickup and refunds the public fare or shop purchase payment; no delivery reward is paid. Missing roads and obstacles still apply.", 12., interface::MUTED));
            interface::heading(p, "Nearby handover");
            line(p, FleetLabel::Recovery, false);
            button(p, "Choose unassigned relief employee", A::NextRelief);
            line(p, FleetLabel::Vehicle, false);
            button(p, "Choose replacement vehicle", A::NextVehicle);
            button(p, "Hand over load to relief crew", A::Handover);
            interface::heading(p, "Roadside support");
            line(p, FleetLabel::Roadside, false);
            button(p, "Buy quoted roadside service", A::RoadsideService);
        });
        section(p, FleetTab::Depot, |p| {
            interface::heading(p, "Your depot bays");
            line(p, FleetLabel::Depot, true);
            button(p, "Browse garage / depot", A::NextDepot);
            button(p, "Open depot bay · 25 company cr", A::OpenDepot);
            interface::pair(
                p,
                ("Choose item", Action::Fleet(A::DepotItem)),
                ("Quantity", Action::Fleet(A::DepotQuantity)),
            );
            interface::pair(
                p,
                ("Store goods", Action::Fleet(A::DepositGoods)),
                ("Take goods", Action::Fleet(A::WithdrawGoods)),
            );
            interface::heading(p, "Trade with a nearby shop");
            line(p, FleetLabel::DepotMarket, false);
            button(p, "Choose shop", A::NextMarket);
            interface::button_with_label(
                p,
                "Buy into depot",
                Action::Fleet(A::BuyDepotGoods),
                Some(Label::Fleet(FleetLabel::DepotBuy)),
            );
            interface::button_with_label(
                p,
                "Sell from depot",
                Action::Fleet(A::SellDepotGoods),
                Some(Label::Fleet(FleetLabel::DepotSell)),
            );
            p.spawn(interface::text("Trades use company funds and real shop stock. Stand on foot within 20 m of both the shop and your bay. For a distant buyer, draft a warehouse sale and dispatch a driver.", 12., interface::MUTED));
            button(p, "Draft warehouse sale to a shop", A::SuggestDepotSale);
            button(
                p,
                "Draft transfer to your other bay",
                A::SuggestDepotTransfer,
            );
            interface::button_with_label(
                p,
                "Close empty depot",
                Action::Fleet(A::CloseDepot),
                Some(Label::Fleet(FleetLabel::CloseDepot)),
            );
            p.spawn(interface::text("Each bay holds 2,000 items, including space reserved for loaded deliveries and returns. Bays at garages waive onsite service callout; freight facilities provide storage only. Delete routes using a bay before closing it.", 12., interface::MUTED));
            interface::heading(p, "A working local economy");
            p.spawn(interface::text("At your first depot, off-duty employee households spend their earned wages on one real shop item and a 0.50 cr transit pass per physical minute. Bread and water purchases create new delivery demand and return money to shops and the service fund. Supplies, earned money and a shop within 200 m are required. Offline companies freeze.", 12., interface::TEXT));
        });
        section(p, FleetTab::Routes, |p| {
            interface::heading(p, "Saved routes");
            line(p, FleetLabel::Route, false);
            button(p, "Choose next saved route", A::NextRoute);
            interface::button_with_label(
                p,
                "Delete unused route",
                Action::Fleet(A::DeleteRoute),
                Some(Label::Fleet(FleetLabel::Delete)),
            );
            interface::heading(p, "Create a route");
            line(p, FleetLabel::Draft, true);
            button(p, "Draft a local passenger loop", A::SuggestRoute);
            button(p, "Draft warehouse sale to a shop", A::SuggestDepotSale);
            button(
                p,
                "Draft transfer between your bays",
                A::SuggestDepotTransfer,
            );
            button(p, "Edit route name", A::EditRouteName);
            interface::pair(
                p,
                ("Service type", Action::Fleet(A::Service)),
                ("Load quantity", Action::Fleet(A::Quantity)),
            );
            button(p, "Choose next freight item", A::Item);
            line(p, FleetLabel::Stop, false);
            button(p, "Browse next stop", A::NextStop);
            button(p, "Append this stop", A::AppendStop);
            interface::pair(
                p,
                ("Remove last stop", Action::Fleet(A::UndoStop)),
                ("Clear draft stops", Action::Fleet(A::ClearStops)),
            );
            button(p, "Create named route", A::SaveRoute);
            interface::heading(p, "Before dispatch");
            line(p, FleetLabel::DraftEconomics, false);
            p.spawn(interface::text("Choose 2–8 distinct stops, each 50 m–2 km from the next, including return. Public freight moves linked industry stock. Warehouse sales load your bay and sell to a shop; the buyer reserves its current purchase price. Warehouse transfers move goods between your bays without a fare. All freight returns empty and uses payroll and fuel.", 12., interface::MUTED));
        });
    });
}
pub fn company(s: &SandboxRuntime) -> Option<&TransportCompany> {
    s.state.transport.companies.get(&s.player_id)
}
fn driver<'a>(s: &'a SandboxRuntime, ui: &UiState) -> Option<&'a CompanyDriver> {
    let company = company(s)?;
    match ui.fleet.driver {
        Some(id) => company.drivers.iter().find(|driver| driver.id == id),
        None => company.drivers.first(),
    }
}
fn route<'a>(s: &'a SandboxRuntime, ui: &UiState) -> Option<&'a CompanyRoute> {
    let company = company(s)?;
    match ui.fleet.route {
        Some(id) => company.routes.iter().find(|route| route.id == id),
        None => company.routes.first(),
    }
}
fn vehicle<'a>(s: &'a SandboxRuntime, ui: &UiState) -> Option<&'a VehicleState> {
    match ui.fleet.vehicle {
        Some(id) => s
            .state
            .transport
            .vehicles
            .get(&id)
            .filter(|vehicle| vehicle.owner == s.player_id),
        None => s
            .state
            .transport
            .vehicles
            .values()
            .find(|vehicle| vehicle.owner == s.player_id),
    }
}
fn stop<'a>(s: &'a SandboxRuntime, ui: &UiState) -> Option<&'a TransportFacility> {
    match ui.fleet.stop {
        Some(id) => s.state.transport.facilities.get(&id),
        None => s.state.transport.facilities.values().next(),
    }
}
fn relief<'a>(s: &'a SandboxRuntime, ui: &UiState) -> Option<&'a CompanyDriver> {
    let current = driver(s, ui).map(|driver| driver.id);
    let company = company(s)?;
    let eligible =
        |driver: &&CompanyDriver| driver.vehicle_id.is_none() && Some(driver.id) != current;
    match ui.fleet.relief {
        Some(id) => company
            .drivers
            .iter()
            .filter(eligible)
            .find(|driver| driver.id == id),
        None => company.drivers.iter().find(eligible),
    }
}
fn depot_facility<'a>(s: &'a SandboxRuntime, ui: &UiState) -> Option<&'a TransportFacility> {
    let eligible = |facility: &&TransportFacility| {
        matches!(
            facility.kind,
            TransportFacilityKind::Garage | TransportFacilityKind::Depot
        )
    };
    match ui.fleet.depot {
        Some(id) => s.state.transport.facilities.get(&id).filter(eligible),
        None => s.state.transport.facilities.values().find(eligible),
    }
}
fn owned_depot<'a>(s: &'a SandboxRuntime, ui: &UiState) -> Option<&'a CompanyDepot> {
    let facility = depot_facility(s, ui)?.id;
    company(s)?
        .depots
        .iter()
        .find(|d| d.facility_id == facility)
}
fn at_depot_facility(s: &SandboxRuntime, ui: &UiState) -> bool {
    let Some(f) = depot_facility(s, ui) else {
        return false;
    };
    s.state.players.get(&s.player_id).is_some_and(|p| {
        p.position
            .surface_distance_m(f.position)
            .hypot(p.position.altitude_m - f.position.altitude_m)
            <= 20.
    })
}
fn depot_transfer_problem(s: &SandboxRuntime, ui: &UiState, deposit: bool) -> Option<&'static str> {
    if !at_depot_facility(s, ui) {
        return Some("Approach this facility within 20 m to move goods.");
    }
    let Some(depot) = owned_depot(s, ui) else {
        return Some("Open a depot bay here first.");
    };
    if s.state
        .living
        .accounts
        .get(&s.player_id)
        .is_none_or(|a| a.active_unit != a.avatar_unit)
    {
        return Some("Exit to your walking avatar to move goods.");
    }
    let item = interface::TRADE_ITEMS[ui.fleet.item % interface::TRADE_ITEMS.len()].0;
    let Some(player) = s.state.players.get(&s.player_id) else {
        return Some("Player inventory is loading.");
    };
    let amount = ui.fleet.quantity;
    let source = if deposit {
        &player.inventory
    } else {
        &depot.stock
    };
    if source.get(item).copied().unwrap_or(0) < amount {
        return Some("The source inventory has too little of this item.");
    }
    if deposit
        && depot.stock.values().map(|n| u64::from(*n)).sum::<u64>()
            + u64::from(amount)
            + s.state
                .company_depot_reserved_units(s.player_id, depot.facility_id)
            > COMPANY_DEPOT_CAPACITY
    {
        return Some("Depot space is full or reserved for a loaded delivery/return.");
    }
    None
}
fn depot_label(s: &SandboxRuntime, ui: &UiState) -> String {
    let Some(f) = depot_facility(s, ui) else {
        return match ui.fleet.depot {
            Some(id) => format!(
                "Selected depot facility #{id} is unavailable in this view. Choose another facility to review its bay."
            ),
            None => {
                "No garage or freight facilities are visible here. Explore another area.".into()
            }
        };
    };
    let item = interface::TRADE_ITEMS[ui.fleet.item % interface::TRADE_ITEMS.len()].0;
    let owned = owned_depot(s, ui);
    let stored = owned.map_or(0, |d| d.stock.get(item).copied().unwrap_or(0));
    let total = owned.map_or(0, |d| d.stock.values().map(|n| u64::from(*n)).sum::<u64>());
    let personal = s
        .state
        .players
        .get(&s.player_id)
        .and_then(|p| p.inventory.get(item))
        .copied()
        .unwrap_or(0);
    format!(
        "{}\n{}\nSelected: {} {}\nPersonal: {} · Stored: {}\nStorage: {} / 2000 units\nReserved delivery/return space: {}\n{}",
        f.name,
        if owned.is_some() {
            "Your private depot bay"
        } else {
            "Bay available · 25 company cr"
        },
        ui.fleet.quantity,
        item,
        personal,
        stored,
        total,
        s.state.company_depot_reserved_units(s.player_id, f.id),
        if at_depot_facility(s, ui) {
            "Facility in reach"
        } else {
            "Approach this facility within 20 m"
        }
    )
}

fn market_shop<'a>(s: &'a SandboxRuntime, ui: &UiState) -> Option<&'a Shop> {
    ui.fleet
        .market_shop
        .and_then(|id| s.state.living.shops.get(&id))
        .or_else(|| {
            let position = depot_facility(s, ui)?.position;
            s.state.living.shops.values().min_by(|a, b| {
                a.position
                    .surface_distance_m(position)
                    .total_cmp(&b.position.surface_distance_m(position))
                    .then(a.id.cmp(&b.id))
            })
        })
}

fn depot_market_quote(
    s: &SandboxRuntime,
    ui: &UiState,
    buy: bool,
) -> Result<CompanyDepotTradeQuote, String> {
    let facility = depot_facility(s, ui).ok_or("Choose a depot facility first.")?;
    let shop = market_shop(s, ui)
        .ok_or("No shop is visible. Move toward a market or use a warehouse sale route.")?;
    let item = interface::TRADE_ITEMS[ui.fleet.item % interface::TRADE_ITEMS.len()].0;
    s.state
        .quote_company_depot_trade(
            s.player_id,
            facility.id,
            shop.id,
            item,
            ui.fleet.quantity,
            buy,
        )
        .map_err(|e| e.to_string())
}

fn depot_market_label(s: &SandboxRuntime, ui: &UiState) -> String {
    let Some(shop) = market_shop(s, ui) else {
        return "No shop in the current view. Explore a market, or deliver by warehouse route."
            .into();
    };
    let item = interface::TRADE_ITEMS[ui.fleet.item % interface::TRADE_ITEMS.len()].0;
    let distance = s.state.players.get(&s.player_id).map_or(0., |p| {
        p.position
            .surface_distance_m(shop.position)
            .hypot(p.position.altitude_m - shop.position.altitude_m)
    });
    let buy = depot_market_quote(s, ui, true)
        .map(|q| {
            format!(
                "Buy {} {} for {} company cr",
                q.amount,
                q.item,
                money(q.total_cost)
            )
        })
        .unwrap_or_else(|e| format!("Buy: {e}"));
    let sell = depot_market_quote(s, ui, false)
        .map(|q| {
            format!(
                "Sell {} {} for {} company cr",
                q.amount,
                q.item,
                money(q.total_cost)
            )
        })
        .unwrap_or_else(|e| format!("Sell: {e}"));
    format!(
        "{} · {:.0} m\nShop stock: {} {}\nShop funds: {} cr\n{}\n{}",
        shop.name,
        distance,
        shop.stock.get(item).copied().unwrap_or(0),
        item,
        money(shop.money),
        buy,
        sell
    )
}
fn handover_problem(s: &SandboxRuntime, ui: &UiState) -> Option<&'static str> {
    let Some(driver) = driver(s, ui).filter(|d| d.trip.is_some()) else {
        return Some("Select an employee with a loaded trip.");
    };
    if relief(s, ui).is_none() {
        return Some("Hire or release a different employee for relief duty.");
    }
    let Some(replacement) = vehicle(s, ui) else {
        return Some("Choose an owned replacement vehicle.");
    };
    if s.state.is_company_vehicle(replacement.unit_id) || !replacement.cargo.is_empty() {
        return Some("Choose a free, empty replacement vehicle.");
    }
    if s.state.transport.jobs.values().any(|j| {
        j.assignment
            .as_ref()
            .is_some_and(|a| a.vehicle_id == replacement.unit_id)
    }) || s
        .state
        .transport
        .races
        .values()
        .any(|r| r.vehicle_id == replacement.unit_id)
    {
        return Some("Finish the replacement vehicle's job or race first.");
    }
    let Some(account) = s.state.living.accounts.get(&s.player_id) else {
        return Some("Player is loading.");
    };
    if account.active_unit != account.avatar_unit {
        return Some("Exit to your walking avatar first.");
    }
    let Some(source) = driver
        .vehicle_id
        .and_then(|id| s.state.living.units.get(&id))
    else {
        return Some("The loaded vehicle is outside the current view.");
    };
    let Some(target) = s.state.living.units.get(&replacement.unit_id) else {
        return Some("The replacement is outside the current view.");
    };
    if !stopped(s, source.id) || !stopped(s, target.id) {
        return Some("Hold the loaded vehicle and stop the replacement.");
    }
    let Some(player) = s.state.players.get(&s.player_id) else {
        return Some("Player is loading.");
    };
    let close = |a: terra_core::GeoPosition, b: terra_core::GeoPosition| {
        a.surface_distance_m(b).hypot(a.altitude_m - b.altitude_m) <= 20.
    };
    if !close(source.position, target.position)
        || !close(player.position, source.position)
        || !close(player.position, target.position)
    {
        return Some("Bring both stopped vehicles and your avatar within 20 m of one another.");
    }
    let Some(route) =
        company(s).and_then(|c| c.routes.iter().find(|r| Some(r.id) == driver.route_id))
    else {
        return Some("The assigned route is unavailable.");
    };
    let fits = match &route.service {
        FleetService::Passengers { count } => *count <= replacement.model.spec().passenger_seats,
        FleetService::Freight { amount, .. }
        | FleetService::DepotFreight { amount, .. }
        | FleetService::DepotTransfer { amount, .. } => {
            *amount <= s.state.vehicle_cargo_capacity(replacement.unit_id)
        }
    };
    if !fits || replacement.model.spec().required_license == DriverLicense::Emergency {
        return Some("Choose a qualified replacement with room for the full route load.");
    }
    None
}

fn next(ids: Vec<u64>, current: Option<u64>) -> Option<u64> {
    if ids.is_empty() {
        None
    } else {
        Some(
            ids[current
                .and_then(|id| ids.iter().position(|v| *v == id))
                .map_or(0, |i| (i + 1) % ids.len())],
        )
    }
}
fn money(value: u64) -> String {
    format!("{}.{:02}", value / 100, value % 100)
}
fn signed_money(value: i64) -> String {
    format!(
        "{}{}",
        if value < 0 { "−" } else { "" },
        money(value.unsigned_abs())
    )
}

fn finance_label(label: FleetLabel, company: Option<&TransportCompany>) -> String {
    let Some(company) = company else {
        return match label {
            FleetLabel::FinanceResult => {
                "Register a company to begin recording income, costs and owner funding.".into()
            }
            _ => "No company records yet.".into(),
        };
    };
    let report = company.accounting_report();
    match label {
        FleetLabel::FinanceResult => format!(
            "{} cr before owner funding\n{} cr after recorded owner-paid costs",
            signed_money(report.cash_result_before_financing),
            signed_money(report.cash_result_after_known_owner_costs),
        ),
        FleetLabel::FinanceCash => format!(
            "Operating receipts: {} cr\nOperating payments: {} cr\nSetup and depot payments: {} cr\nOpening cash: {} cr\nCurrent cash: {} cr\n{}",
            money(report.operating.receipts),
            money(report.operating.payments),
            money(report.capital.payments),
            money(report.opening_cash),
            money(report.cash),
            if report.reconciled {
                "All recorded cash movements reconcile."
            } else {
                "Records do not reconcile. Restore a verified checkpoint before trading."
            },
        ),
        FleetLabel::FinanceFunding => format!(
            "Deposited into company: {} cr\nWithdrawn by owner: {} cr\nPaid personally for operations: {} cr\nPaid personally for setup: {} cr\nKnown owner funding before withdrawals: {} cr\nDeposits add cash; they do not count as earnings.",
            money(report.financing.receipts),
            money(report.financing.payments),
            money(report.owner_paid_operating),
            money(report.owner_paid_capital),
            money(report.known_owner_funding),
        ),
        FleetLabel::FinanceCategories => {
            let rows: Vec<_> = report
                .totals
                .iter()
                .filter(|(_, amount)| **amount != 0)
                .map(|(category, amount)| format!("{}: {} cr", category.label(), money(*amount)))
                .collect();
            if rows.is_empty() {
                "No recorded payments or receipts.".into()
            } else {
                rows.join("\n")
            }
        }
        FleetLabel::FinanceScope => {
            let coverage = match report.coverage {
                CompanyAccountingCoverage::SinceRegistration => {
                    "Records cover this company since registration.".to_string()
                }
                CompanyAccountingCoverage::ImportedOpeningBalance => format!(
                    "This company began in an older world. Records start from its imported balance; earlier costs are unknown. Historical revenue: {} cr; historical payroll: {} cr.",
                    money(report.opening_revenue),
                    money(report.opening_payroll),
                ),
            };
            let stock = if report.unpriced_stock_in_units != 0
                || report.unpriced_stock_out_units != 0
            {
                format!(
                    "\nOwner-supplied stock: {} items in, {} out. Its purchase or production cost is not known.",
                    report.unpriced_stock_in_units, report.unpriced_stock_out_units
                )
            } else {
                String::new()
            };
            let vehicle = if report.personal_vehicle_used {
                "\nA personal vehicle has worked for this company. Its acquisition and costs before assignment are not included."
            } else {
                ""
            };
            format!(
                "{coverage}{stock}{vehicle}\nThis is a cash report. It does not value goods, vehicles, unpaid time or depreciation; it is not a full profit calculation."
            )
        }
        FleetLabel::FinanceEntries => {
            if report.recent_entries.is_empty() {
                return "No recorded payments or receipts.".into();
            }
            let mut rows: Vec<_> = report
                .recent_entries
                .iter()
                .rev()
                .take(8)
                .map(|entry| {
                    let payer = if entry.category.owner_paid() {
                        "personal payment"
                    } else if entry.category.receipt() {
                        "company receipt"
                    } else {
                        "company payment"
                    };
                    format!(
                        "#{} · {}\n{} cr · {payer}\nCompany cash after: {} cr",
                        entry.sequence,
                        entry.category.label(),
                        money(entry.amount),
                        money(entry.cash_after)
                    )
                })
                .collect();
            rows.push(format!(
                "Showing the latest {} entries. Category totals cover all {} recorded entries.",
                rows.len(),
                report.monetary_entry_count
            ));
            rows.join("\n\n")
        }
        _ => String::new(),
    }
}
fn service(ui: &UiState) -> FleetService {
    let item = interface::TRADE_ITEMS[ui.fleet.item % interface::TRADE_ITEMS.len()]
        .0
        .into();
    let amount = ui.fleet.quantity;
    match ui.fleet.service_kind {
        FleetDraftKind::Passengers => FleetService::Passengers {
            count: amount.min(30) as u16,
        },
        FleetDraftKind::PublicFreight => FleetService::Freight { item, amount },
        FleetDraftKind::WarehouseSale => FleetService::DepotFreight { item, amount },
        FleetDraftKind::WarehouseTransfer => FleetService::DepotTransfer { item, amount },
    }
}
fn service_label(service: &FleetService) -> String {
    match service {
        FleetService::Passengers { count } => format!("{count} passenger(s) per leg"),
        FleetService::Freight { item, amount } => format!("Public freight · up to {amount} {item}"),
        FleetService::DepotFreight { item, amount } => {
            format!("Warehouse sale · up to {amount} {item}")
        }
        FleetService::DepotTransfer { item, amount } => {
            format!("Warehouse transfer · up to {amount} {item} · no fare")
        }
    }
}

fn route_economics(s: &SandboxRuntime, stops: &[u64], service: &FleetService) -> String {
    if stops.len() < 2 {
        return "Add two stops to compare receipts, stock and payroll before creating the route."
            .into();
    }
    let Ok(preview) = s.state.company_route_preview(s.player_id, stops, service) else {
        return "Complete a valid route to review its first departure. Missing stops may be outside the current view.".into();
    };
    let source = &s.state.transport.facilities[&preview.pickup];
    let target = &s.state.transport.facilities[&preview.destination];
    let mut lines = vec![format!(
        "First departure: {} → {}",
        source.name, target.name
    )];
    match service {
        FleetService::Passengers { .. } => {
            lines.push(format!(
                "Queue now: {} · requested: {} people",
                source.passengers_waiting, preview.requested_load
            ));
        }
        _ => {
            let (item, _) = service.cargo_load().unwrap();
            lines.push(
                preview
                    .visible_pickup_units
                    .map(|n| {
                        format!(
                            "Pickup stock: {n} {item} · requested: {}",
                            preview.requested_load
                        )
                    })
                    .unwrap_or_else(|| {
                        "Pickup stock unavailable here. Check the source and your bay access."
                            .into()
                    }),
            );
            if let (Some(stock), Some(target)) = (
                preview.visible_destination_units,
                preview.destination_target_units,
            ) {
                lines.push(if stock >= target {
                    "No restocking demand at the buyer now. Wait for consumption or choose another destination.".into()
                } else {
                    format!("Destination stock: {stock} / {target} target; incoming loads may fill the gap.")
                });
            }
        }
    }
    if matches!(service, FleetService::DepotTransfer { .. }) {
        lines.push("No payment for a bay transfer. Budget payroll, fuel and the empty return; check destination space.".into());
    } else {
        lines.push(preview.full_load_receipt.map(|receipt| format!(
            "Full-load receipt: {} cr, only after funded delivery.", money(receipt)))
            .unwrap_or_else(|| "Buyer price is unavailable in this view. Inspect the destination before dispatch.".into()));
        if let Some(retail) = &preview.retail_comparison {
            let shop = &s.state.living.shops[&retail.shop_id];
            lines.push(format!(
                "Retail at {}: {} cr each. At that price: {} cr for {}; {} available.",
                shop.name,
                money(retail.unit_price),
                money(retail.full_load_cost),
                preview.requested_load,
                retail.available_units
            ));
            if let Some(margin) = retail.merchandise_margin {
                lines.push(if margin < 0 {
                    format!("Buying at that price leaves a {} cr shortfall before payroll or fuel. Use a costed source or choose other work.", money(margin.unsigned_abs()))
                } else {
                    format!("After that stock purchase: {} cr before payroll, fuel and other costs.", money(margin as u64))
                });
            }
        }
        if let Some(seconds) = preview.receipt_payroll_seconds() {
            lines.push(format!("Gross receipt alone covers at most {:.1} minutes of payroll at 1.20 cr/min, before all other costs.", seconds / 60.));
        }
        if preview.public_fare {
            lines.push("Service funding is finite; the issuer must reserve the fare at pickup. Vehicle costs and your time remain separate.".into());
        } else {
            lines.push("Contributed goods and your time remain unpriced. Retail comparison does not include acquisition travel or vehicle costs.".into());
        }
    }
    lines.push("Current view only; prices, stock, funding and road delays may change. Review actual results in Finances.".into());
    lines.join("\n")
}

fn suggested_depot_route(
    s: &SandboxRuntime,
    ui: &UiState,
    transfer: bool,
) -> Result<(u64, u64, usize), String> {
    let c = company(s).ok_or("Register a company and open a bay first.")?;
    let mut depots: Vec<_> = c.depots.iter().collect();
    depots.sort_by_key(|d| {
        (
            Some(d.facility_id) != depot_facility(s, ui).map(|f| f.id),
            d.facility_id,
        )
    });
    for depot in depots {
        let Some(source) = s.state.transport.facilities.get(&depot.facility_id) else {
            continue;
        };
        let mut items: Vec<_> = interface::TRADE_ITEMS.iter().enumerate().collect();
        items.sort_by_key(|(i, (id, _))| {
            (
                depot.stock.get(*id).copied().unwrap_or(0) == 0,
                *i != ui.fleet.item,
                *i,
            )
        });
        for (item_index, (item, _)) in items {
            let destination = s
                .state
                .transport
                .facilities
                .values()
                .filter(|f| {
                    let distance = source.position.surface_distance_m(f.position);
                    f.id != source.id
                        && (50. ..=2000.).contains(&distance)
                        && if transfer {
                            c.depots.iter().any(|d| d.facility_id == f.id)
                        } else {
                            f.linked_site
                                .and_then(|id| s.state.living.shops.get(&id))
                                .is_some_and(|shop| shop.prices.contains_key(*item))
                        }
                })
                .min_by(|a, b| {
                    a.position
                        .surface_distance_m(source.position)
                        .total_cmp(&b.position.surface_distance_m(source.position))
                        .then(a.id.cmp(&b.id))
                });
            if let Some(destination) = destination {
                return Ok((source.id, destination.id, item_index));
            }
        }
    }
    Err(if transfer { "Open two bays 50 m–2 km apart before drafting a transfer." } else { "Open a bay 50 m–2 km from a buying shop. Freight facilities also offer bays; their public stock remains separate." }.into())
}
fn at_garage(s: &SandboxRuntime) -> bool {
    s.state.players.get(&s.player_id).is_some_and(|p| {
        s.state.transport.facilities.values().any(|g| {
            g.kind == TransportFacilityKind::Garage
                && p.position
                    .surface_distance_m(g.position)
                    .hypot(p.position.altitude_m - g.position.altitude_m)
                    <= 20.
        })
    })
}
fn stopped(s: &SandboxRuntime, id: u64) -> bool {
    s.state
        .living
        .units
        .get(&id)
        .is_some_and(|u| u.velocity_enu_m_s[0].hypot(u.velocity_enu_m_s[1]) <= 0.5)
}
fn route_problem(s: &SandboxRuntime, ui: &UiState) -> Option<String> {
    if ui.fleet.route_name.trim().is_empty() || ui.fleet.route_name.chars().count() > 32 {
        return Some("Choose a route name of 1–32 characters.".into());
    }
    let stops = &ui.fleet.draft_stops;
    if !(2..=8).contains(&stops.len()) {
        return Some("Append at least two stops before creating the route.".into());
    }
    for (i, id) in stops.iter().enumerate() {
        let Some(a) = s.state.transport.facilities.get(id) else {
            return Some("A stop is no longer available. Clear the draft and choose again.".into());
        };
        let Some(b) = s
            .state
            .transport
            .facilities
            .get(&stops[(i + 1) % stops.len()])
        else {
            return Some("A stop is no longer available.".into());
        };
        let distance = a.position.surface_distance_m(b.position);
        if !(50. ..=2_000.).contains(&distance) {
            return Some(format!(
                "{} → {} is {:.0} m; each leg must be 50 m–2 km.",
                a.name, b.name, distance
            ));
        }
    }
    if let FleetService::Freight { item, .. } = service(ui) {
        let first = &s.state.transport.facilities[&stops[0]];
        let last = &s.state.transport.facilities[stops.last().unwrap()];
        if first.linked_site.is_none()
            || last.linked_site.is_none()
            || first.linked_site == last.linked_site
        {
            return Some(
                "Choose different linked inventories for freight pickup and delivery.".into(),
            );
        }
        if s.state.fleet_destination_target(last.id, &item) == 0 {
            return Some(format!(
                "{} does not consume or sell {item}. Choose another item or destination.",
                last.name
            ));
        }
    }
    if service(ui).uses_company_stock() {
        let Some(c) = company(s) else {
            return Some("Register a company and open a warehouse bay first.".into());
        };
        if !c.depots.iter().any(|d| d.facility_id == stops[0]) {
            return Some("The first stop must be one of your owned warehouse bays.".into());
        }
        let destination = *stops.last().unwrap();
        if matches!(service(ui), FleetService::DepotTransfer { .. }) {
            if !c.depots.iter().any(|d| d.facility_id == destination) {
                return Some("A warehouse transfer must end at your other owned bay.".into());
            }
        } else {
            let item = interface::TRADE_ITEMS[ui.fleet.item % interface::TRADE_ITEMS.len()].0;
            if s.state.transport.facilities[&destination]
                .linked_site
                .and_then(|id| s.state.living.shops.get(&id))
                .is_none_or(|shop| !shop.prices.contains_key(item))
            {
                return Some(
                    "Warehouse sales must end at a shop that buys the selected item.".into(),
                );
            }
        }
    }
    if company(s).is_some_and(|c| {
        c.routes
            .iter()
            .any(|r| r.name.eq_ignore_ascii_case(&ui.fleet.route_name))
    }) {
        return Some("Choose a different name; that route already exists.".into());
    }
    None
}
fn assignment_problem(s: &SandboxRuntime, ui: &UiState) -> Option<&'static str> {
    let Some(d) = driver(s, ui) else {
        return Some(if ui.fleet.driver.is_some() {
            "The selected employee is unavailable. Choose another employee."
        } else {
            "Hire a driver at a garage first."
        });
    };
    if d.vehicle_id.is_some() {
        return Some("This driver already has a vehicle. Pause and release it before reassigning.");
    }
    let Some(v) = vehicle(s, ui) else {
        return Some(if ui.fleet.vehicle.is_some() {
            "The selected vehicle is unavailable or no longer yours. Choose another owned vehicle."
        } else {
            "Purchase or choose an owned vehicle."
        });
    };
    let Some(r) = route(s, ui) else {
        return Some(if ui.fleet.route.is_some() {
            "The selected route is unavailable. Choose another saved route."
        } else {
            "Create a route in Routes first."
        });
    };
    if s.state
        .living
        .accounts
        .get(&s.player_id)
        .is_some_and(|a| a.active_unit == v.unit_id)
    {
        return Some("Exit the vehicle and select your avatar before assigning it.");
    }
    if !stopped(s, v.unit_id) || !v.cargo.is_empty() {
        return Some("Stop and unload the selected vehicle.");
    }
    if s.state.is_company_vehicle(v.unit_id) {
        return Some("The vehicle is assigned to another employee.");
    }
    if s.state.transport.jobs.values().any(|j| {
        j.assignment
            .as_ref()
            .is_some_and(|a| a.vehicle_id == v.unit_id)
    }) || s
        .state
        .transport
        .races
        .values()
        .any(|r| r.vehicle_id == v.unit_id)
    {
        return Some("Finish the vehicle's job or race first.");
    }
    if v.model.spec().required_license == DriverLicense::Emergency {
        return Some(
            "Choose a passenger or goods vehicle; employees have no emergency qualification.",
        );
    }
    let fits = match &r.service {
        FleetService::Passengers { count } => *count <= v.model.spec().passenger_seats,
        FleetService::Freight { amount, .. }
        | FleetService::DepotFreight { amount, .. }
        | FleetService::DepotTransfer { amount, .. } => {
            *amount <= s.state.vehicle_cargo_capacity(v.unit_id)
        }
    };
    if !fits {
        return Some("The vehicle has too little capacity for this route.");
    }
    None
}
fn status_label(status: FleetStatus) -> &'static str {
    match status {
        FleetStatus::DestinationFull => "Delivery waiting — travel or capacity",
        FleetStatus::RouteBlocked => "Route blocked",
        _ => status.label(),
    }
}
fn next_stop_label(s: &SandboxRuntime, driver: &CompanyDriver) -> String {
    let returning = driver
        .trip
        .as_ref()
        .filter(|t| t.returning)
        .map(|t| t.pickup);
    let next_id = returning.or_else(|| {
        company(s)
            .and_then(|c| c.routes.iter().find(|r| Some(r.id) == driver.route_id))
            .and_then(|route| route.stops.get(driver.next_stop))
            .copied()
    });
    let next = next_id.and_then(|id| s.state.transport.facilities.get(&id));
    let Some(stop) = next else {
        return "Next stop: assign a route".into();
    };
    let distance = driver
        .vehicle_id
        .and_then(|id| s.state.living.units.get(&id))
        .map(|unit| unit.position.surface_distance_m(stop.position))
        .map(|metres| {
            if metres < 1_000. {
                format!("{metres:.0} m")
            } else {
                format!("{:.2} km", metres / 1_000.)
            }
        })
        .unwrap_or_else(|| "vehicle outside current view".into());
    format!(
        "{}: {}\nDirect distance: {distance}",
        if returning.is_some() {
            "Return pickup"
        } else {
            "Next stop"
        },
        stop.name
    )
}
fn freight_stop_label(s: &SandboxRuntime, stop: &TransportFacility, item: &str) -> String {
    let site = stop.linked_site;
    let producer = site.and_then(|id| s.state.living.producers.get(&id));
    let stock = producer
        .map(|p| &p.stock)
        .or_else(|| site.and_then(|id| s.state.living.shops.get(&id).map(|shop| &shop.stock)));
    let Some(stock) = stock else {
        return if site.is_some() {
            "Linked inventory is outside the current view.".into()
        } else {
            "Freight needs a stop with a linked inventory.".into()
        };
    };
    let quantity = stock.get(item).copied().unwrap_or(0);
    let target = s.state.fleet_destination_target(stop.id, item);
    let demand = if target == 0 {
        format!("No delivery demand for {item} here")
    } else {
        format!("Delivery target: {target} {item}")
    };
    let capacity = producer
        .map(|p| {
            format!(
                "\nStorage: {} / 2000 units",
                p.stock.values().map(|n| u64::from(*n)).sum::<u64>()
            )
        })
        .unwrap_or_default();
    format!(
        "In stock: {quantity} {item}\n{demand}{capacity}\nReservations can reduce available stock and demand."
    )
}
fn roadside_offer(s: &SandboxRuntime, ui: &UiState) -> Result<CompanyRoadsideQuote, String> {
    let vehicle = driver(s, ui)
        .and_then(|d| d.vehicle_id)
        .and_then(|id| s.state.transport.vehicles.get(&id))
        .ok_or_else(|| "Assign a vehicle to the selected driver first.".to_owned())?;
    let body = s
        .state
        .living
        .units
        .get(&vehicle.unit_id)
        .ok_or_else(|| "The assigned vehicle is outside the current view.".to_owned())?;
    let desired_litres = (vehicle.model.spec().fuel_capacity_l - vehicle.fuel_l)
        .floor()
        .clamp(0., 10.) as u16;
    let repair = vehicle.condition < 0.9999;
    if desired_litres == 0 && !repair {
        return Err("The assigned vehicle needs no whole litres of fuel or repairs.".into());
    }
    let garage = s
        .state
        .transport
        .facilities
        .values()
        .filter(|f| f.kind == TransportFacilityKind::Garage)
        .filter(|f| desired_litres == 0 || f.fuel_l >= 1. || repair)
        .min_by(|a, b| {
            a.position
                .surface_distance_m(body.position)
                .total_cmp(&b.position.surface_distance_m(body.position))
                .then(a.id.cmp(&b.id))
        })
        .ok_or_else(|| {
            if repair {
                "No supplying garage is available for repairs.".to_owned()
            } else {
                "No supplying garage has fuel available for this callout.".to_owned()
            }
        })?;
    let litres = desired_litres.min(garage.fuel_l.floor().max(0.) as u16);
    s.state
        .company_roadside_quote(s.player_id, vehicle.unit_id, garage.id, litres, repair)
        .map_err(|error| error.to_string())
}
fn roadside_problem(s: &SandboxRuntime, quote: &CompanyRoadsideQuote) -> Option<String> {
    let Some(body) = s.state.living.units.get(&quote.vehicle_id) else {
        return Some("The assigned vehicle is outside the current view.".into());
    };
    let Some(player) = s.state.players.get(&s.player_id) else {
        return Some("The player is still loading.".into());
    };
    let distance = player
        .position
        .surface_distance_m(body.position)
        .hypot(player.position.altitude_m - body.position.altitude_m);
    if distance > 20. {
        return Some(format!(
            "Approach the assigned vehicle: {distance:.0} m away; service reach is 20 m."
        ));
    }
    if !stopped(s, quote.vehicle_id) {
        return Some("Wait until the assigned vehicle has stopped.".into());
    }
    let cash = company(s).map_or(0, |c| c.cash);
    if cash < quote.total_cost {
        return Some(format!(
            "Deposit {} more company cr for this service.",
            money(quote.total_cost - cash)
        ));
    }
    if s.state
        .transport
        .treasury
        .checked_add(quote.total_cost)
        .is_none_or(|cash| cash > MAX_MONEY)
    {
        return Some("The service treasury has reached its account limit.".into());
    }
    None
}
pub fn label(label: FleetLabel, s: &SandboxRuntime, ui: &UiState) -> String {
    let c = company(s);
    match label {
        FleetLabel::Summary => c.map(|c|format!("{}\nCompany cash: {} cr\nEmployees: {} / 4 · Routes: {} / 8",c.name,money(c.cash),c.drivers.len(),c.routes.len())).unwrap_or_else(||"Put your vehicles to work with hired drivers.".into()),
        FleetLabel::Navigation => format!("{} · {}",ui.fleet.tab.title(), if ui.fleet.navigation_open { "hide sections" } else { "choose section" }),
        FleetLabel::FinanceResult | FleetLabel::FinanceCash | FleetLabel::FinanceFunding | FleetLabel::FinanceCategories | FleetLabel::FinanceScope | FleetLabel::FinanceEntries => finance_label(label,c),
        FleetLabel::FundServices => if confirmation_current(FleetAction::FundServices,s,ui) {format!("Confirm {} cr public contribution",money(ui.fleet.transfer))} else {format!("Fund public services · {} cr",money(ui.fleet.transfer))},
        FleetLabel::Registration => if c.is_some(){"Company registered. Manage its funds, drivers and routes here.".into()}else if at_garage(s){"Garage in reach · Registration costs 20 personal cr.".into()}else{"Visit a garage within 20 m to register or recruit.".into()},
        FleetLabel::Transfer => format!("Transfer {} cr between your personal and company accounts",money(ui.fleet.transfer)),
        FleetLabel::Driver => driver(s,ui).map(|d|format!("{}\n{}\nDeliveries: {}\nHousehold wallet: {} cr\n{}\n{}",d.name,status_label(d.status),d.deliveries_completed,money(d.wages_earned),next_stop_label(s,d),d.trip.as_ref().map(|t|format!("Loaded: {}\nReserved payment: {} cr",service_label(&t.service),money(t.reward_escrow + t.goods_payment.as_ref().map_or(0, |p| p.total)))).unwrap_or_else(||"No passengers or freight aboard".into()))).unwrap_or_else(||match ui.fleet.driver { Some(id)=>format!("Selected employee #{id} is unavailable. Choose next employee to review another."), None=>"No employees yet. Deposit company funds and recruit at a garage.".into() }),
        FleetLabel::Vehicle => vehicle(s,ui).map(|v|format!("{} · {:.1} L fuel · {:.0}% condition\n{} seats · {} cargo capacity{}",v.model.spec().name,v.fuel_l,v.condition*100.,v.model.spec().passenger_seats,s.state.vehicle_cargo_capacity(v.unit_id),if s.state.is_company_vehicle(v.unit_id){" · Assigned"}else{""})).unwrap_or_else(||match ui.fleet.vehicle { Some(id)=>format!("Selected vehicle #{id} is unavailable or no longer yours. Choose another owned vehicle."), None=>"No owned vehicle available.".into() }),
        FleetLabel::Route => route(s,ui).map(|r|format!("{} · {}\n{}",r.name,service_label(&r.service),r.stops.iter().filter_map(|id|s.state.transport.facilities.get(id).map(|f|f.name.clone())).collect::<Vec<_>>().join(" → "))).unwrap_or_else(||match ui.fleet.route { Some(id)=>format!("Selected route #{id} is unavailable. Choose next saved route to review another."), None=>"No saved routes. Build a route below or open Routes.".into() }),
        FleetLabel::RouteEconomics => route(s,ui).map(|r|route_economics(s,&r.stops,&r.service)).unwrap_or_else(||"Choose a saved route to review its first departure.".into()),
        FleetLabel::Draft => format!("{}\n{}\n{}\n{}",ui.fleet.route_name,service_label(&service(ui)),if ui.fleet.draft_stops.is_empty(){"No stops added".into()}else{ui.fleet.draft_stops.iter().enumerate().map(|(i,id)|format!("{}. {}",i+1,s.state.transport.facilities.get(id).map(|f|f.name.as_str()).unwrap_or("Missing stop"))).collect::<Vec<_>>().join("\n")},route_problem(s,ui).unwrap_or_else(||"Route is ready to create.".into())),
        FleetLabel::DraftEconomics => route_economics(s,&ui.fleet.draft_stops,&service(ui)),
        FleetLabel::Stop => stop(s,ui).map(|f| {
            let details = match service(ui) {
                FleetService::Freight { item, .. } => freight_stop_label(s,f,&item),
                FleetService::DepotFreight { item, .. } | FleetService::DepotTransfer { item, .. } => {
                    let stock = c.and_then(|c| c.depots.iter().find(|d| d.facility_id == f.id)).map(|d| d.stock.get(&item).copied().unwrap_or(0));
                    let buyer = f.linked_site.and_then(|id| s.state.living.shops.get(&id)).and_then(|shop| shop.prices.get(&item));
                    format!("{}\n{}", stock.map(|n|format!("Your warehouse: {n} {item}")).unwrap_or_else(||"No owned warehouse at this stop".into()), buyer.map(|p|format!("Shop buys {item} at {} cr each", money(p.sell))).unwrap_or_else(||"No shop buyer for this item".into()))
                }
                FleetService::Passengers { .. } => format!("Waiting passengers: {}",f.passengers_waiting),
            };
            format!("{} · {:?}\n{details}",f.name,f.kind)
        }).unwrap_or_else(||match ui.fleet.stop {Some(id)=>format!("Selected stop #{id} is unavailable in this view. Browse another stop."),None=>"Service stops are still preparing.".into()}),
        FleetLabel::Prerequisites => if let Some(d)=driver(s,ui).filter(|d|d.vehicle_id.is_some()) {
            let recovery = match d.status {
                FleetStatus::RouteBlocked if d.trip.is_none() => "\nWait for traffic or clear the path, then Start / resume. Payroll is held; rental time continues. To change route, Pause safely then Release.",
                FleetStatus::RouteBlocked => "\nWait for traffic or clear the path, then resume in Recovery. Payroll is held; rental time continues. Recovery also offers load return or a nearby handover.",
                FleetStatus::RentalExpired => "\nOpen Garage, select this rental and renew its prepaid time. The load, bond and company assignment stay intact.",
                FleetStatus::Held => "\nPayroll is stopped. Open Recovery to resume, return or hand over this load.",
                FleetStatus::DestinationFull => "\nLoad and payment are retained. Check destination space, company cash capacity, buyer refund capacity and required physical travel.",
                FleetStatus::AccountingFull => "\nCompany records have reached their lifetime limit. Payroll, load and reserved payment remain held. A load already at its original pickup can be returned; further paid travel needs accounting capacity.",
                FleetStatus::WaitingBuyerFunds => "\nThe destination shop must earn purchase funds from real customer sales. No goods have been loaded; this wait does not charge payroll.",
                FleetStatus::WaitingGoods => "\nCheck the first stop: warehouse routes use your private bay. Store gathered or crafted goods, buy stock at a nearby market, or change the route.",
                FleetStatus::WaitingDemand => "\nThe destination already has stock or incoming deliveries. Customer consumption or withdrawals create room for another load.",
                FleetStatus::NeedsFuel | FleetStatus::NeedsRepair => "\nApproach within 20 m of the stopped vehicle for the quoted roadside service below. Its load and route stay intact.",
                _ => "",
            };
            format!("Assigned: {} → {}\n{}{recovery}",d.vehicle_id.and_then(|id|s.state.transport.vehicles.get(&id)).map(|v|v.model.spec().name).unwrap_or("Vehicle outside view"),c.and_then(|c|c.routes.iter().find(|r|Some(r.id)==d.route_id)).map(|r|r.name.as_str()).unwrap_or("Missing route"),status_label(d.status))
        }else{assignment_problem(s,ui).map(str::to_owned).unwrap_or_else(||"Ready to assign. Start the route after assigning and funding payroll.".into())},
        FleetLabel::Roadside => match roadside_offer(s,ui) {
            Ok(quote) => format!("Supply: {}\nCallout: {} cr\nFuel: {} L · {} cr\nRepairs: {}\nTotal from company: {} cr\n{}\nCargo, payment and route are retained.",
                s.state.transport.facilities.get(&quote.facility_id).map(|f|f.name.as_str()).unwrap_or("Garage"),
                money(quote.callout_cost),quote.litres,money(quote.fuel_cost),
                if quote.repair{format!("{} cr",money(quote.repair_cost))}else{"not needed".into()},
                money(quote.total_cost),roadside_problem(s,&quote).unwrap_or_else(||"In reach and funded. Review the quote, then buy service.".into())),
            Err(message) => message,
        },
        FleetLabel::Recovery => format!("Relief employee: {}\n{}", relief(s,ui).map(|d|d.name.clone()).unwrap_or_else(||match ui.fleet.relief {Some(id)=>format!("selected employee #{id} is unavailable; choose another relief employee"),None=>"hire or release another employee".into()}), handover_problem(s,ui).unwrap_or("Ready: load and reserved payment move once. Vehicle positions remain unchanged; the relief driver stays held until resumed.")),
        FleetLabel::Depot => depot_label(s,ui),
        FleetLabel::DepotMarket => depot_market_label(s,ui),
        FleetLabel::DepotBuy => depot_market_quote(s,ui,true).map(|q|format!("Buy into depot · {} cr", money(q.total_cost))).unwrap_or_else(|_|"Buy into depot · unavailable".into()),
        FleetLabel::DepotSell => depot_market_quote(s,ui,false).map(|q|format!("Sell from depot · {} cr", money(q.total_cost))).unwrap_or_else(|_|"Sell from depot · unavailable".into()),
        FleetLabel::Dismiss => if confirmation_current(FleetAction::Dismiss,s,ui) {driver(s,ui).map(|d|format!("Confirm dismiss {} #{} · earned wages kept",d.name,d.id)).unwrap_or_default()}else{"Dismiss unassigned employee".into()},
        FleetLabel::Return => if confirmation_current(FleetAction::Return,s,ui) {driver(s,ui).map(|d|format!("Confirm return {} #{}'s load · refund on arrival",d.name,d.id)).unwrap_or_default()}else{"Return load to original pickup".into()},
        FleetLabel::CloseDepot => if confirmation_current(FleetAction::CloseDepot,s,ui) {depot_facility(s,ui).map(|d|format!("Confirm close {} #{} · opening fee not refunded",d.name,d.id)).unwrap_or_default()}else{"Close empty depot".into()},
        FleetLabel::Delete => if confirmation_current(FleetAction::DeleteRoute,s,ui) {route(s,ui).map(|r|format!("Confirm delete {} #{}",r.name,r.id)).unwrap_or_default()}else{"Delete unused route".into()},
    }
}
pub fn act(action: FleetAction, ui: &mut UiState, s: &mut SandboxRuntime) -> Option<Command> {
    use FleetAction as A;
    let mut command = None;
    if ui
        .fleet
        .confirmation
        .as_ref()
        .is_some_and(|review| !review.target.matches_action(action))
    {
        ui.fleet.confirmation = None;
    }
    match action {
        A::Tab(tab) => {
            ui.fleet.tab = tab;
            ui.fleet.navigation_open = false;
        }
        A::ToggleNavigation => ui.fleet.navigation_open = !ui.fleet.navigation_open,
        A::Register | A::Hire | A::EditRouteName => {
            ui.input_mode = match action {
                A::Register => InputMode::CompanyName,
                A::Hire => InputMode::DriverName,
                _ => InputMode::RouteName,
            };
            ui.gps_text = if matches!(action, A::EditRouteName) {
                ui.fleet.route_name.clone()
            } else {
                String::new()
            };
            ui.editing_gps = true;
        }
        A::TransferAmount => {
            ui.fleet.transfer = match ui.fleet.transfer {
                1_000 => 5_000,
                5_000 => 10_000,
                _ => 1_000,
            }
        }
        A::Deposit => {
            command = Some(CompanyCommand::Deposit {
                amount: ui.fleet.transfer,
            })
        }
        A::FundServices | A::Return | A::Dismiss | A::CloseDepot | A::DeleteRoute => {
            command = confirmed_action(action, ui, s)
        }
        A::Withdraw => {
            command = Some(CompanyCommand::Withdraw {
                amount: ui.fleet.transfer,
            })
        }
        A::NextDriver => {
            ui.fleet.driver = next(
                company(s)
                    .map(|c| c.drivers.iter().map(|d| d.id).collect())
                    .unwrap_or_default(),
                driver(s, ui).map(|d| d.id),
            )
        }
        A::NextVehicle => {
            ui.fleet.vehicle = next(
                s.state
                    .transport
                    .vehicles
                    .values()
                    .filter(|v| v.owner == s.player_id)
                    .map(|v| v.unit_id)
                    .collect(),
                vehicle(s, ui).map(|v| v.unit_id),
            )
        }
        A::NextRoute => {
            ui.fleet.route = next(
                company(s)
                    .map(|c| c.routes.iter().map(|r| r.id).collect())
                    .unwrap_or_default(),
                route(s, ui).map(|r| r.id),
            )
        }
        A::Assign => {
            if let (Some(d), Some(v), Some(r)) = (driver(s, ui), vehicle(s, ui), route(s, ui)) {
                command = Some(CompanyCommand::Assign {
                    driver_id: d.id,
                    vehicle_id: v.unit_id,
                    route_id: r.id,
                });
            }
        }
        A::Run | A::Pause => {
            if let Some(d) = driver(s, ui) {
                command = Some(CompanyCommand::SetDriverRunning {
                    driver_id: d.id,
                    running: matches!(action, A::Run),
                });
            }
        }
        A::Release => {
            if let Some(d) = driver(s, ui) {
                command = Some(CompanyCommand::ReleaseVehicle { driver_id: d.id });
            }
        }
        A::Hold => {
            if let Some(d) = driver(s, ui) {
                command = Some(CompanyCommand::HoldVehicle { driver_id: d.id });
            }
        }
        A::NextRelief => {
            ui.fleet.relief = next(
                company(s)
                    .map(|c| {
                        c.drivers
                            .iter()
                            .filter(|d| {
                                d.vehicle_id.is_none() && Some(d.id) != driver(s, ui).map(|d| d.id)
                            })
                            .map(|d| d.id)
                            .collect()
                    })
                    .unwrap_or_default(),
                relief(s, ui).map(|d| d.id),
            )
        }
        A::Handover => {
            if let (Some(d), Some(r), Some(v)) = (driver(s, ui), relief(s, ui), vehicle(s, ui)) {
                command = Some(CompanyCommand::HandoverLoad {
                    driver_id: d.id,
                    relief_driver_id: r.id,
                    vehicle_id: v.unit_id,
                });
            }
        }
        A::NextDepot => {
            ui.fleet.depot = next(
                s.state
                    .transport
                    .facilities
                    .values()
                    .filter(|f| {
                        matches!(
                            f.kind,
                            TransportFacilityKind::Garage | TransportFacilityKind::Depot
                        )
                    })
                    .map(|f| f.id)
                    .collect(),
                depot_facility(s, ui).map(|f| f.id),
            )
        }
        A::OpenDepot => {
            if let Some(f) = depot_facility(s, ui) {
                command = Some(CompanyCommand::OpenDepot { facility_id: f.id });
            }
        }
        A::DepotItem => ui.fleet.item = (ui.fleet.item + 1) % interface::TRADE_ITEMS.len(),
        A::DepotQuantity => {
            ui.fleet.quantity = match ui.fleet.quantity {
                1 => 4,
                4 => 8,
                8 => 16,
                16 => 32,
                _ => 1,
            }
        }
        A::DepositGoods | A::WithdrawGoods => {
            if let Some(f) = depot_facility(s, ui) {
                command = Some(CompanyCommand::TransferDepotStock {
                    facility_id: f.id,
                    item: interface::TRADE_ITEMS[ui.fleet.item % interface::TRADE_ITEMS.len()]
                        .0
                        .into(),
                    amount: ui.fleet.quantity,
                    deposit: matches!(action, A::DepositGoods),
                });
            }
        }
        A::NextMarket => {
            ui.fleet.market_shop = next(
                s.state.living.shops.keys().copied().collect(),
                market_shop(s, ui).map(|shop| shop.id),
            );
        }
        A::BuyDepotGoods | A::SellDepotGoods => {
            match reviewed_depot_quote(s, ui, matches!(action, A::BuyDepotGoods)) {
                Ok(quote) => {
                    command = Some(CompanyCommand::TradeDepotStock {
                        facility_id: quote.facility_id,
                        shop_id: quote.shop_id,
                        item: quote.item,
                        amount: quote.amount,
                        buy: quote.buy,
                        quoted_total: quote.total_cost,
                    })
                }
                Err(reason) => s.last_message = reason,
            }
        }
        A::SuggestDepotSale | A::SuggestDepotTransfer => {
            let transfer = matches!(action, A::SuggestDepotTransfer);
            match suggested_depot_route(s, ui, transfer) {
                Ok((source, destination, item)) => {
                    ui.fleet.service_kind = if transfer {
                        FleetDraftKind::WarehouseTransfer
                    } else {
                        FleetDraftKind::WarehouseSale
                    };
                    ui.fleet.draft_stops = vec![source, destination];
                    ui.fleet.item = item;
                    ui.fleet.quantity = 4;
                    ui.fleet.route_name = if transfer {
                        "Warehouse transfer"
                    } else {
                        "Warehouse supply"
                    }
                    .into();
                    ui.fleet.tab = FleetTab::Routes;
                    s.last_message=if transfer { "Private transfer drafted. No public fare is paid; fund payroll, review and create the route." } else { "Warehouse sale drafted. Your stock sells at the buyer's purchase price when physically delivered. Review and create the route." }.into();
                }
                Err(reason) => s.last_message = reason,
            }
        }
        A::RoadsideService => match reviewed_roadside_quote(s, ui) {
            Ok(quote) => {
                if let Some(problem) = roadside_problem(s, &quote) {
                    s.last_message = problem;
                } else {
                    command = Some(CompanyCommand::RoadsideService {
                        vehicle_id: quote.vehicle_id,
                        facility_id: quote.facility_id,
                        litres: quote.litres,
                        repair: quote.repair,
                        quoted_total: quote.total_cost,
                    });
                }
            }
            Err(message) => s.last_message = message,
        },
        A::SuggestRoute => {
            let stops: Vec<_> = s
                .state
                .transport
                .facilities
                .values()
                .filter(|f| f.passengers_waiting > 0 && f.kind == TransportFacilityKind::Terminal)
                .collect();
            let pair = stops.iter().enumerate().find_map(|(i, a)| {
                stops
                    .iter()
                    .skip(i + 1)
                    .find(|b| (50. ..=2000.).contains(&a.position.surface_distance_m(b.position)))
                    .map(|b| (a.id, b.id))
            });
            if let Some((a, b)) = pair {
                ui.fleet.draft_stops = vec![a, b];
                ui.fleet.service_kind = FleetDraftKind::Passengers;
                ui.fleet.quantity = 1;
                ui.fleet.route_name = "Town service".into();
                s.last_message="Passenger loop drafted from available queues. Edit its name or stops, then create the route.".into();
            } else {
                s.last_message="No compatible pair of stops with waiting passengers. Browse stops to build your own route.".into();
            }
        }
        A::Service => {
            ui.fleet.service_kind = match ui.fleet.service_kind {
                FleetDraftKind::Passengers => FleetDraftKind::PublicFreight,
                FleetDraftKind::PublicFreight => FleetDraftKind::WarehouseSale,
                FleetDraftKind::WarehouseSale => FleetDraftKind::WarehouseTransfer,
                FleetDraftKind::WarehouseTransfer => FleetDraftKind::Passengers,
            };
            ui.fleet.quantity = 1;
        }
        A::Item => ui.fleet.item = (ui.fleet.item + 1) % interface::TRADE_ITEMS.len(),
        A::Quantity => {
            ui.fleet.quantity = match ui.fleet.quantity {
                1 => 2,
                2 if ui.fleet.service_kind == FleetDraftKind::Passengers => 3,
                2 | 3 => 4,
                4 => 8,
                8 => 16,
                16 if ui.fleet.service_kind == FleetDraftKind::Passengers => 30,
                16 => 32,
                _ => 1,
            }
        }
        A::NextStop => {
            ui.fleet.stop = next(
                s.state.transport.facilities.keys().copied().collect(),
                stop(s, ui).map(|f| f.id),
            )
        }
        A::AppendStop => {
            if let Some(id) = stop(s, ui).map(|f| f.id) {
                if ui.fleet.draft_stops.len() < 8 && !ui.fleet.draft_stops.contains(&id) {
                    ui.fleet.draft_stops.push(id);
                }
            }
        }
        A::UndoStop => {
            ui.fleet.draft_stops.pop();
        }
        A::ClearStops => ui.fleet.draft_stops.clear(),
        A::SaveRoute => {
            if let Some(reason) = route_problem(s, ui) {
                s.last_message = reason;
            } else {
                command = Some(CompanyCommand::CreateRoute {
                    name: ui.fleet.route_name.trim().into(),
                    stops: ui.fleet.draft_stops.clone(),
                    service: service(ui),
                });
            }
        }
    }
    command.map(|action| Command::Transport {
        action: TransportCommand::Company { action },
    })
}
pub fn unavailable(action: FleetAction, s: &SandboxRuntime, ui: &UiState) -> bool {
    use FleetAction as A;
    let c = company(s);
    let wallet = s
        .state
        .living
        .accounts
        .get(&s.player_id)
        .map_or(0, |a| a.money);
    match action {
        A::SuggestRoute | A::NextDepot | A::DepotItem | A::DepotQuantity => false,
        A::SuggestDepotSale | A::SuggestDepotTransfer => c.is_none_or(|c| c.depots.is_empty()),
        A::NextMarket => s.state.living.shops.len() < 2,
        A::BuyDepotGoods | A::SellDepotGoods => {
            depot_market_quote(s, ui, matches!(action, A::BuyDepotGoods)).is_err()
        }
        A::Dismiss => !at_garage(s) || driver(s, ui).is_none_or(|d| d.vehicle_id.is_some()),
        A::Hold => {
            driver(s, ui).is_none_or(|d| d.vehicle_id.is_none() || d.status == FleetStatus::Held)
        }
        A::Return => driver(s, ui).is_none_or(|d| d.trip.as_ref().is_none_or(|t| t.returning)),
        A::NextRelief => company(s).is_none_or(|c| {
            c.drivers
                .iter()
                .filter(|d| {
                    d.vehicle_id.is_none() && Some(d.id) != driver(s, ui).map(|driver| driver.id)
                })
                .count()
                < if ui.fleet.relief.is_some() && relief(s, ui).is_none() {
                    1
                } else {
                    2
                }
        }),
        A::Handover => handover_problem(s, ui).is_some(),
        A::OpenDepot => {
            !at_depot_facility(s, ui)
                || c.is_none_or(|c| {
                    c.cash < COMPANY_DEPOT_PRICE
                        || c.depots.len() >= MAX_COMPANY_DEPOTS
                        || depot_facility(s, ui)
                            .is_some_and(|f| c.depots.iter().any(|d| d.facility_id == f.id))
                })
        }
        A::CloseDepot => {
            !at_depot_facility(s, ui)
                || owned_depot(s, ui).is_none_or(|d| {
                    !d.stock.is_empty()
                        || c.is_some_and(|c| {
                            c.routes.iter().any(|r| {
                                r.service.uses_company_stock()
                                    && (r.stops.first() == Some(&d.facility_id)
                                        || r.stops.last() == Some(&d.facility_id))
                            })
                        })
                })
        }
        A::DepositGoods | A::WithdrawGoods => {
            depot_transfer_problem(s, ui, matches!(action, A::DepositGoods)).is_some()
        }
        A::Tab(_)
        | A::ToggleNavigation
        | A::TransferAmount
        | A::EditRouteName
        | A::Service
        | A::Quantity
        | A::NextStop => false,
        A::Register => c.is_some() || !at_garage(s) || wallet < COMPANY_REGISTRATION_PRICE,
        A::Hire => {
            !at_garage(s)
                || c.is_none_or(|c| {
                    c.cash < COMPANY_HIRE_PRICE || c.drivers.len() >= MAX_COMPANY_DRIVERS
                })
        }
        A::Deposit => c.is_none_or(|c| {
            wallet < ui.fleet.transfer
                || c.cash
                    .checked_add(ui.fleet.transfer)
                    .is_none_or(|v| v > MAX_MONEY)
        }),
        A::Withdraw => c.is_none_or(|c| {
            c.cash < ui.fleet.transfer
                || wallet
                    .checked_add(ui.fleet.transfer)
                    .is_none_or(|v| v > MAX_MONEY)
        }),
        A::FundServices => c.is_none_or(|c| c.cash < ui.fleet.transfer),
        A::NextDriver => c.is_none_or(|c| {
            c.drivers.len()
                < if ui.fleet.driver.is_some() && driver(s, ui).is_none() {
                    1
                } else {
                    2
                }
        }),
        A::NextVehicle => {
            s.state
                .transport
                .vehicles
                .values()
                .filter(|v| v.owner == s.player_id)
                .count()
                < if ui.fleet.vehicle.is_some() && vehicle(s, ui).is_none() {
                    1
                } else {
                    2
                }
        }
        A::NextRoute => c.is_none_or(|c| {
            c.routes.len()
                < if ui.fleet.route.is_some() && route(s, ui).is_none() {
                    1
                } else {
                    2
                }
        }),
        A::Assign => assignment_problem(s, ui).is_some(),
        A::Run => {
            c.is_none_or(|c| c.cash == 0)
                || driver(s, ui).is_none_or(|d| {
                    d.vehicle_id.is_none()
                        || d.vehicle_id
                            .and_then(|id| s.state.transport.vehicles.get(&id))
                            .is_none_or(|vehicle| {
                                vehicle.owner != s.player_id
                                    || vehicle
                                        .rental
                                        .as_ref()
                                        .is_some_and(|rental| rental.remaining_seconds <= 0.)
                            })
                        || (d.running
                            && !matches!(d.status, FleetStatus::RouteBlocked | FleetStatus::Held))
                })
        }
        A::Pause => driver(s, ui).is_none_or(|d| !d.running),
        A::Release => driver(s, ui).is_none_or(|d| {
            d.running || d.trip.is_some() || d.vehicle_id.is_none_or(|id| !stopped(s, id))
        }),
        A::RoadsideService => {
            roadside_offer(s, ui).map_or(true, |quote| roadside_problem(s, &quote).is_some())
        }
        A::Item => ui.fleet.service_kind == FleetDraftKind::Passengers,
        A::AppendStop => {
            ui.fleet.draft_stops.len() >= 8
                || stop(s, ui).is_none_or(|f| ui.fleet.draft_stops.contains(&f.id))
        }
        A::UndoStop | A::ClearStops => ui.fleet.draft_stops.is_empty(),
        A::SaveRoute => {
            c.is_none_or(|c| c.routes.len() >= MAX_COMPANY_ROUTES) || route_problem(s, ui).is_some()
        }
        A::DeleteRoute => route(s, ui)
            .is_none_or(|r| c.is_none_or(|c| c.drivers.iter().any(|d| d.route_id == Some(r.id)))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> SandboxRuntime {
        let origin = terra_core::GeoPosition::new(48.137, 11.575, 100.).unwrap();
        let mut s = SandboxRuntime::new(origin);
        s.state.seed_living_demo(origin, &[], &s.catalog).unwrap();
        s.state.ensure_transport_demo(&s.catalog).unwrap();
        let garage = s
            .state
            .transport
            .facilities
            .values()
            .find(|f| {
                matches!(
                    f.kind,
                    TransportFacilityKind::Garage | TransportFacilityKind::Depot
                )
            })
            .unwrap()
            .position;
        s.state.set_player_position(s.player_id, garage).unwrap();
        s
    }
    fn apply(s: &mut SandboxRuntime, command: CompanyCommand) {
        s.state
            .apply(
                s.player_id,
                Command::Transport {
                    action: TransportCommand::Company { action: command },
                },
                &s.catalog,
            )
            .unwrap();
    }

    fn confirmation_fixture() -> (SandboxRuntime, UiState) {
        let mut s = fixture();
        let mut ui = UiState {
            panel: Panel::Fleet,
            ..Default::default()
        };
        apply(
            &mut s,
            CompanyCommand::Create {
                name: "Reviewable Transit".into(),
            },
        );
        apply(&mut s, CompanyCommand::Deposit { amount: 15_000 });
        for name in ["Rowan", "Ash"] {
            apply(&mut s, CompanyCommand::HireDriver { name: name.into() });
        }
        act(FleetAction::SuggestRoute, &mut ui, &mut s);
        assert!(route_problem(&s, &ui).is_none());
        for name in ["River route", "Hill route"] {
            apply(
                &mut s,
                CompanyCommand::CreateRoute {
                    name: name.into(),
                    stops: ui.fleet.draft_stops.clone(),
                    service: service(&ui),
                },
            );
        }
        let garage = depot_facility(&s, &ui).unwrap().id;
        let depot = s
            .state
            .transport
            .facilities
            .values()
            .find(|f| f.kind == TransportFacilityKind::Depot)
            .unwrap()
            .id;
        for id in [garage, depot] {
            s.state
                .set_player_position(s.player_id, s.state.transport.facilities[&id].position)
                .unwrap();
            apply(&mut s, CompanyCommand::OpenDepot { facility_id: id });
        }
        s.state
            .set_player_position(s.player_id, s.state.transport.facilities[&garage].position)
            .unwrap();
        ui.fleet.depot = Some(garage);
        ui.fleet.route = Some(company(&s).unwrap().routes[0].id);
        ui.fleet.driver = Some(company(&s).unwrap().drivers[0].id);
        (s, ui)
    }

    #[test]
    fn missing_explicit_fleet_selections_never_borrow_another_target() {
        let (mut s, mut ui) = confirmation_fixture();
        ui.fleet.driver = Some(u64::MAX);
        ui.fleet.route = Some(u64::MAX);
        ui.fleet.vehicle = Some(u64::MAX);
        ui.fleet.stop = Some(u64::MAX);
        ui.fleet.relief = Some(u64::MAX);
        ui.fleet.depot = Some(u64::MAX);
        assert!(driver(&s, &ui).is_none());
        assert!(route(&s, &ui).is_none());
        assert!(vehicle(&s, &ui).is_none());
        assert!(stop(&s, &ui).is_none());
        assert!(relief(&s, &ui).is_none());
        assert!(depot_facility(&s, &ui).is_none());
        assert!(label(FleetLabel::Driver, &s, &ui).contains("unavailable"));
        assert!(label(FleetLabel::Vehicle, &s, &ui).contains("unavailable"));
        assert!(label(FleetLabel::Route, &s, &ui).contains("unavailable"));
        assert!(label(FleetLabel::Stop, &s, &ui).contains("unavailable"));
        assert!(label(FleetLabel::Depot, &s, &ui).contains("unavailable"));
        for action in [
            FleetAction::NextDriver,
            FleetAction::NextRoute,
            FleetAction::NextVehicle,
            FleetAction::NextStop,
            FleetAction::NextRelief,
            FleetAction::NextDepot,
        ] {
            assert!(!unavailable(action, &s, &ui));
            assert!(act(action, &mut ui, &mut s).is_none());
        }
        assert!(driver(&s, &ui).is_some());
        assert!(route(&s, &ui).is_some());
        assert!(vehicle(&s, &ui).is_some());
        assert!(stop(&s, &ui).is_some());
        assert!(relief(&s, &ui).is_some());
        assert!(depot_facility(&s, &ui).is_some());
    }

    #[test]
    fn destructive_fleet_reviews_do_not_survive_restore_or_replay_confirmation() {
        for action in [
            FleetAction::FundServices,
            FleetAction::Dismiss,
            FleetAction::CloseDepot,
            FleetAction::DeleteRoute,
        ] {
            let (mut s, mut ui) = confirmation_fixture();
            let before = s.state.clone();
            assert!(act(action, &mut ui, &mut s).is_none());
            assert!(confirmation_current(action, &s, &ui));
            assert_eq!(s.state, before);
            s.restore_revision += 1;
            assert!(!confirmation_current(action, &s, &ui));
            assert!(
                act(action, &mut ui, &mut s).is_none(),
                "restore requires a new review, even when the target still has the same ID"
            );
            assert_eq!(s.state, before);
            let command = act(action, &mut ui, &mut s).unwrap();
            assert!(ui.fleet.confirmation.is_none());
            s.state.apply(s.player_id, command, &s.catalog).unwrap();
            assert!(
                act(action, &mut ui, &mut s).is_none(),
                "a third click cannot replay the confirmed command"
            );
        }
    }

    #[test]
    fn vanished_route_or_employee_requires_deliberate_browsing_then_its_own_review() {
        for action in [FleetAction::DeleteRoute, FleetAction::Dismiss] {
            let (mut s, mut ui) = confirmation_fixture();
            let original_route = route(&s, &ui).unwrap().id;
            let original_driver = driver(&s, &ui).unwrap().id;
            assert!(act(action, &mut ui, &mut s).is_none());
            match action {
                FleetAction::DeleteRoute => apply(
                    &mut s,
                    CompanyCommand::DeleteRoute {
                        route_id: original_route,
                    },
                ),
                _ => apply(
                    &mut s,
                    CompanyCommand::DismissDriver {
                        driver_id: original_driver,
                    },
                ),
            }
            let before = s.state.clone();
            assert!(act(action, &mut ui, &mut s).is_none());
            assert_eq!(
                s.state, before,
                "a missing explicit target never transfers its named review"
            );
            assert!(act(action, &mut ui, &mut s).is_none());
            let browse = match action {
                FleetAction::DeleteRoute => {
                    assert!(route(&s, &ui).is_none());
                    assert_eq!(ui.fleet.route, Some(original_route));
                    FleetAction::NextRoute
                }
                _ => {
                    assert!(driver(&s, &ui).is_none());
                    assert_eq!(ui.fleet.driver, Some(original_driver));
                    FleetAction::NextDriver
                }
            };
            assert!(act(browse, &mut ui, &mut s).is_none());
            assert!(act(action, &mut ui, &mut s).is_none());
            assert_eq!(
                s.state, before,
                "the deliberately chosen target needs review"
            );
            let command = act(action, &mut ui, &mut s).unwrap();
            match command {
                Command::Transport {
                    action:
                        TransportCommand::Company {
                            action: CompanyCommand::DeleteRoute { route_id },
                        },
                } => assert_ne!(route_id, original_route),
                Command::Transport {
                    action:
                        TransportCommand::Company {
                            action: CompanyCommand::DismissDriver { driver_id },
                        },
                } => assert_ne!(driver_id, original_driver),
                _ => panic!("unexpected fleet action"),
            }
        }
    }

    #[test]
    fn contribution_amount_depot_and_player_changes_require_fresh_fleet_review() {
        let (mut s, mut ui) = confirmation_fixture();
        assert!(act(FleetAction::FundServices, &mut ui, &mut s).is_none());
        ui.fleet.transfer = 1_000;
        assert!(act(FleetAction::FundServices, &mut ui, &mut s).is_none());
        assert!(matches!(
            act(FleetAction::FundServices, &mut ui, &mut s),
            Some(Command::Transport {
                action: TransportCommand::Company {
                    action: CompanyCommand::FundPublicServices { amount: 1_000 }
                }
            })
        ));
        assert!(act(FleetAction::CloseDepot, &mut ui, &mut s).is_none());
        let other = company(&s)
            .unwrap()
            .depots
            .iter()
            .find(|d| Some(d.facility_id) != ui.fleet.depot)
            .unwrap()
            .facility_id;
        ui.fleet.depot = Some(other);
        assert!(act(FleetAction::CloseDepot, &mut ui, &mut s).is_none());
        assert!(
            matches!(act(FleetAction::CloseDepot, &mut ui, &mut s), Some(Command::Transport { action: TransportCommand::Company { action: CompanyCommand::CloseDepot { facility_id } } }) if facility_id == other)
        );

        // A different registered principal may own an independent company with
        // overlapping local numeric IDs. Its money needs a separate review.
        let position = s.state.players[&1].position;
        s.state.add_player(2, position).unwrap();
        s.state.ensure_living_player(2).unwrap();
        s.state.ensure_transport_demo(&s.catalog).unwrap();
        s.player_id = 2;
        apply(
            &mut s,
            CompanyCommand::Create {
                name: "Other Transit".into(),
            },
        );
        apply(&mut s, CompanyCommand::Deposit { amount: 5_000 });
        s.player_id = 1;
        assert!(act(FleetAction::FundServices, &mut ui, &mut s).is_none());
        s.player_id = 2;
        let before = s.state.clone();
        assert!(act(FleetAction::FundServices, &mut ui, &mut s).is_none());
        assert_eq!(s.state, before);
        assert!(act(FleetAction::FundServices, &mut ui, &mut s).is_some());
    }

    #[test]
    fn returning_a_load_binds_the_actual_trip_and_vehicle_not_just_the_driver() {
        let (mut s, mut ui) = confirmation_fixture();
        let vehicle = *s.state.transport.vehicles.keys().next().unwrap();
        let stops = route(&s, &ui).unwrap().stops.clone();
        let driver_id = driver(&s, &ui).unwrap().id;
        // This test isolates the native review boundary; domain return tests
        // separately prove cargo, escrow and physical arrival conservation.
        let driver = s
            .state
            .transport
            .companies
            .get_mut(&1)
            .unwrap()
            .drivers
            .iter_mut()
            .find(|d| d.id == driver_id)
            .unwrap();
        driver.vehicle_id = Some(vehicle);
        driver.trip = Some(FleetTrip {
            pickup: stops[0],
            destination: stops[1],
            service: FleetService::Passengers { count: 1 },
            reward_escrow: 500,
            pickup_odometer_m: 20.,
            pickup_elapsed_seconds: 10.,
            returning: false,
            transferred_distance_m: 0.,
            goods_payment: None,
        });
        assert!(act(FleetAction::Return, &mut ui, &mut s).is_none());
        s.state.transport.companies.get_mut(&1).unwrap().drivers[0]
            .trip
            .as_mut()
            .unwrap()
            .pickup_elapsed_seconds = 30.;
        assert!(
            act(FleetAction::Return, &mut ui, &mut s).is_none(),
            "a replacement load needs a new review even on the same route and vehicle"
        );
        let command = act(FleetAction::Return, &mut ui, &mut s).unwrap();
        assert!(
            matches!(command, Command::Transport { action: TransportCommand::Company { action: CompanyCommand::ReturnLoad { driver_id: target } } } if target == driver_id)
        );
        assert!(act(FleetAction::Return, &mut ui, &mut s).is_none());
        s.restore_revision += 1;
        assert!(act(FleetAction::Return, &mut ui, &mut s).is_none());
        act(FleetAction::NextDriver, &mut ui, &mut s);
        assert!(
            ui.fleet.confirmation.is_none(),
            "another Fleet action cancels the review"
        );
    }

    #[test]
    fn finances_distinguish_owner_funding_and_known_costs_from_cash_earnings() {
        let mut s = fixture();
        let ui = UiState::default();
        apply(
            &mut s,
            CompanyCommand::Create {
                name: "Measured Transit".into(),
            },
        );
        apply(&mut s, CompanyCommand::Deposit { amount: 5_000 });
        apply(
            &mut s,
            CompanyCommand::HireDriver {
                name: "Rowan".into(),
            },
        );
        let result = label(FleetLabel::FinanceResult, &s, &ui);
        assert!(
            result.contains("−10.00 cr before owner funding"),
            "{result}"
        );
        assert!(
            result.contains("−30.00 cr after recorded owner-paid costs"),
            "{result}"
        );
        assert!(
            label(FleetLabel::FinanceFunding, &s, &ui).contains("Deposited into company: 50.00 cr")
        );
        assert!(label(FleetLabel::FinanceCash, &s, &ui).contains("Current cash: 40.00 cr"));
        assert!(label(FleetLabel::FinanceCash, &s, &ui).contains("reconcile"));
        let entries = label(FleetLabel::FinanceEntries, &s, &ui);
        assert!(entries.contains("personal payment"));
        assert!(entries.contains("company receipt"));
        assert!(entries.contains("company payment"));

        // Legacy company counters remain historical; the view cannot invent
        // earlier costs or treat imported cash as new recorded earnings.
        s.state
            .transport
            .companies
            .get_mut(&s.player_id)
            .unwrap()
            .accounting = None;
        assert!(label(FleetLabel::FinanceScope, &s, &ui).contains("earlier costs are unknown"));
        assert!(label(FleetLabel::FinanceCash, &s, &ui).contains("Opening cash: 40.00 cr"));
        assert!(
            label(FleetLabel::FinanceResult, &s, &ui).starts_with("0.00 cr before owner funding")
        );
        assert_eq!(
            label(FleetLabel::FinanceEntries, &s, &ui),
            "No recorded payments or receipts."
        );
    }
    #[test]
    fn roadside_selects_an_eligible_garage_and_can_offer_repairs_when_fuel_is_empty() {
        let mut s = fixture();
        let mut ui = UiState::default();
        apply(
            &mut s,
            CompanyCommand::Create {
                name: "Repair crew".into(),
            },
        );
        apply(&mut s, CompanyCommand::Deposit { amount: 5_000 });
        apply(
            &mut s,
            CompanyCommand::HireDriver {
                name: "Rowan".into(),
            },
        );
        act(FleetAction::SuggestRoute, &mut ui, &mut s);
        let command = act(FleetAction::SaveRoute, &mut ui, &mut s).unwrap();
        s.state.apply(s.player_id, command, &s.catalog).unwrap();
        let command = act(FleetAction::Assign, &mut ui, &mut s).unwrap();
        s.state.apply(s.player_id, command, &s.catalog).unwrap();
        let vehicle = driver(&s, &ui).unwrap().vehicle_id.unwrap();
        let depot = s
            .state
            .transport
            .facilities
            .values()
            .find(|f| f.kind == TransportFacilityKind::Depot)
            .unwrap()
            .position;
        s.state.living.units.get_mut(&vehicle).unwrap().position = depot;
        s.state
            .transport
            .vehicles
            .get_mut(&vehicle)
            .unwrap()
            .condition = 0.9;
        let quote = roadside_offer(&s, &ui).unwrap();
        assert_eq!(
            s.state.transport.facilities[&quote.facility_id].kind,
            TransportFacilityKind::Garage
        );
        assert!(quote.repair);

        for garage in s
            .state
            .transport
            .facilities
            .values_mut()
            .filter(|f| f.kind == TransportFacilityKind::Garage)
        {
            garage.fuel_l = 0.;
        }
        s.state.transport.vehicles.get_mut(&vehicle).unwrap().fuel_l = 0.;
        let repair_only = roadside_offer(&s, &ui).unwrap();
        assert_eq!(repair_only.litres, 0);
        assert!(repair_only.repair);
        s.state.set_player_position(s.player_id, depot).unwrap();
        ui.panel = Panel::Fleet;
        ui.fleet.tab = FleetTab::Recovery;
        refresh_displayed_quotes(&s, &mut ui);
        let before = company(&s).unwrap().cash;
        assert!(
            before >= repair_only.total_cost,
            "fixture must fund the quoted repair"
        );
        let command = act(FleetAction::RoadsideService, &mut ui, &mut s)
            .unwrap_or_else(|| panic!("{}", s.last_message));
        s.state.apply(s.player_id, command, &s.catalog).unwrap();
        assert_eq!(s.state.transport.vehicles[&vehicle].condition, 1.);
        assert_eq!(s.state.transport.vehicles[&vehicle].fuel_l, 0.);
        assert_eq!(company(&s).unwrap().cash, before - repair_only.total_cost);
        assert!(
            roadside_offer(&s, &ui)
                .unwrap_err()
                .contains("fuel available")
        );
    }
    #[test]
    fn native_quantities_can_create_and_assign_the_measured_three_seat_compact_career() {
        let mut s = fixture();
        let mut ui = UiState {
            panel: Panel::Fleet,
            ..Default::default()
        };
        apply(
            &mut s,
            CompanyCommand::Create {
                name: "Compact service".into(),
            },
        );
        apply(&mut s, CompanyCommand::Deposit { amount: 5_000 });
        apply(
            &mut s,
            CompanyCommand::HireDriver {
                name: "Robin".into(),
            },
        );
        let garage = s
            .state
            .transport
            .facilities
            .values()
            .find(|f| f.kind == TransportFacilityKind::Garage)
            .unwrap()
            .id;
        let compact = s
            .state
            .apply(
                s.player_id,
                Command::Transport {
                    action: TransportCommand::RentVehicle {
                        model: VehicleModel::Compact,
                        facility_id: garage,
                    },
                },
                &s.catalog,
            )
            .unwrap()
            .created_id
            .unwrap();
        ui.fleet.vehicle = Some(compact);
        act(FleetAction::SuggestRoute, &mut ui, &mut s);
        act(FleetAction::Quantity, &mut ui, &mut s);
        act(FleetAction::Quantity, &mut ui, &mut s);
        assert!(matches!(
            service(&ui),
            FleetService::Passengers { count: 3 }
        ));
        let create = act(FleetAction::SaveRoute, &mut ui, &mut s).unwrap();
        s.state.apply(s.player_id, create, &s.catalog).unwrap();
        ui.fleet.tab = FleetTab::Drivers;
        assert!(!unavailable(FleetAction::Assign, &s, &ui));
        let assign = act(FleetAction::Assign, &mut ui, &mut s).unwrap();
        s.state.apply(s.player_id, assign, &s.catalog).unwrap();
        assert_eq!(driver(&s, &ui).unwrap().vehicle_id, Some(compact));
        assert!(matches!(
            route(&s, &ui).unwrap().service,
            FleetService::Passengers { count: 3 }
        ));
        // The same control can reach the existing bus capacity without ever
        // emitting a passenger quantity outside the route's 1..=30 contract.
        ui.fleet.tab = FleetTab::Routes;
        let mut bus_capacity_reachable = false;
        for _ in 0..7 {
            act(FleetAction::Quantity, &mut ui, &mut s);
            assert!((1..=30).contains(&ui.fleet.quantity));
            bus_capacity_reachable |=
                ui.fleet.quantity == u32::from(VehicleModel::Bus.spec().passenger_seats);
        }
        assert!(bus_capacity_reachable);
        s.state.validate_with_catalog(&s.catalog).unwrap();
    }

    #[test]
    fn suggested_native_route_uses_real_stops_and_all_commands_share_authority() {
        let mut s = fixture();
        let mut ui = UiState::default();
        assert!(!unavailable(FleetAction::Register, &s, &ui));
        apply(
            &mut s,
            CompanyCommand::Create {
                name: "Native Fleet".into(),
            },
        );
        let before = s.state.living.accounts[&s.player_id].money;
        let command = act(FleetAction::Deposit, &mut ui, &mut s).unwrap();
        s.state.apply(s.player_id, command, &s.catalog).unwrap();
        assert_eq!(
            s.state.living.accounts[&s.player_id].money,
            before - ui.fleet.transfer
        );
        assert_eq!(company(&s).unwrap().cash, ui.fleet.transfer);
        apply(
            &mut s,
            CompanyCommand::HireDriver {
                name: "Rowan".into(),
            },
        );
        act(FleetAction::SuggestRoute, &mut ui, &mut s);
        assert_eq!(ui.fleet.draft_stops.len(), 2);
        assert!(route_problem(&s, &ui).is_none());
        assert!(!unavailable(FleetAction::SaveRoute, &s, &ui));
        let command = act(FleetAction::SaveRoute, &mut ui, &mut s).unwrap();
        s.state.apply(s.player_id, command, &s.catalog).unwrap();
        assert!(route_problem(&s, &ui).unwrap().contains("name"));
        assert!(!unavailable(FleetAction::Assign, &s, &ui));
        let command = act(FleetAction::Assign, &mut ui, &mut s).unwrap();
        s.state.apply(s.player_id, command, &s.catalog).unwrap();
        assert!(label(FleetLabel::Prerequisites, &s, &ui).contains("Assigned:"));
        assert!(!unavailable(FleetAction::Run, &s, &ui));
        let command = act(FleetAction::Run, &mut ui, &mut s).unwrap();
        s.state.apply(s.player_id, command, &s.catalog).unwrap();
        assert!(unavailable(FleetAction::Run, &s, &ui));
        s.state
            .transport
            .companies
            .get_mut(&s.player_id)
            .unwrap()
            .drivers[0]
            .status = FleetStatus::RouteBlocked;
        assert!(!unavailable(FleetAction::Run, &s, &ui));
        assert!(label(FleetLabel::Prerequisites, &s, &ui).contains("resume"));

        // Roadside support follows the selected employee's assignment and its
        // actual physical reach, while retaining a blocked route for explicit
        // resume instead of silently changing work or cargo.
        let body = driver(&s, &ui).unwrap().vehicle_id.unwrap();
        let roadside = terra_core::LocalFrame::new(s.state.living.origin.unwrap())
            .to_geo([500., 0., -500.])
            .unwrap();
        s.state.living.units.get_mut(&body).unwrap().position = roadside;
        s.state.transport.vehicles.get_mut(&body).unwrap().fuel_l = 0.;
        let quote = roadside_offer(&s, &ui).unwrap();
        assert!(unavailable(FleetAction::RoadsideService, &s, &ui));
        assert!(act(FleetAction::RoadsideService, &mut ui, &mut s).is_none());
        s.state
            .set_player_position(
                s.player_id,
                terra_core::LocalFrame::new(roadside)
                    .to_geo([0., 0., 10.])
                    .unwrap(),
            )
            .unwrap();
        assert!(!unavailable(FleetAction::RoadsideService, &s, &ui));
        let cash = company(&s).unwrap().cash;
        let station = s.state.transport.facilities[&quote.facility_id].fuel_l;
        ui.panel = Panel::Fleet;
        ui.fleet.tab = FleetTab::Recovery;
        refresh_displayed_quotes(&s, &mut ui);
        // A changed supplying stock changes the requested service as well as
        // its price. Neither can be replaced silently at click time.
        s.state
            .transport
            .facilities
            .get_mut(&quote.facility_id)
            .unwrap()
            .fuel_l = 1.;
        let before = s.state.clone();
        assert!(act(FleetAction::RoadsideService, &mut ui, &mut s).is_none());
        assert!(s.last_message.contains("quote changed"));
        assert_eq!(s.state, before);
        s.state
            .transport
            .facilities
            .get_mut(&quote.facility_id)
            .unwrap()
            .fuel_l = station;
        refresh_displayed_quotes(&s, &mut ui);
        let command = act(FleetAction::RoadsideService, &mut ui, &mut s).unwrap();
        s.state.apply(s.player_id, command, &s.catalog).unwrap();
        assert_eq!(company(&s).unwrap().cash, cash - quote.total_cost);
        assert_eq!(
            s.state.transport.facilities[&quote.facility_id].fuel_l,
            station - f64::from(quote.litres)
        );
        assert_eq!(
            s.state.transport.vehicles[&body].fuel_l,
            f64::from(quote.litres)
        );
        assert_eq!(s.state.living.units[&body].position, roadside);
        assert_eq!(driver(&s, &ui).unwrap().status, FleetStatus::RouteBlocked);
        assert!(!unavailable(FleetAction::Run, &s, &ui));
    }
    #[test]
    fn native_route_builder_bounds_draft_and_rejects_invalid_freight_endpoints() {
        let mut s = fixture();
        let mut ui = UiState::default();
        act(FleetAction::AppendStop, &mut ui, &mut s);
        act(FleetAction::AppendStop, &mut ui, &mut s);
        assert_eq!(ui.fleet.draft_stops.len(), 1);
        assert!(unavailable(FleetAction::AppendStop, &s, &ui));
        assert!(unavailable(FleetAction::SaveRoute, &s, &ui));
        act(FleetAction::SuggestRoute, &mut ui, &mut s);
        ui.fleet.service_kind = FleetDraftKind::PublicFreight;
        assert!(route_problem(&s, &ui).is_some());
        act(FleetAction::ClearStops, &mut ui, &mut s);
        assert!(unavailable(FleetAction::UndoStop, &s, &ui));
        assert!(label(FleetLabel::Draft, &s, &ui).contains("at least two"));
    }
    #[test]
    fn recovery_ui_holds_loaded_trip_and_confirms_physical_return_without_free_payment() {
        let mut s = fixture();
        let mut ui = UiState::default();
        apply(
            &mut s,
            CompanyCommand::Create {
                name: "Recovery demo".into(),
            },
        );
        apply(&mut s, CompanyCommand::Deposit { amount: 5000 });
        apply(&mut s, CompanyCommand::HireDriver { name: "Ari".into() });
        act(FleetAction::SuggestRoute, &mut ui, &mut s);
        let route = act(FleetAction::SaveRoute, &mut ui, &mut s).unwrap();
        s.state.apply(s.player_id, route, &s.catalog).unwrap();
        let assign = act(FleetAction::Assign, &mut ui, &mut s).unwrap();
        s.state.apply(s.player_id, assign, &s.catalog).unwrap();
        let run = act(FleetAction::Run, &mut ui, &mut s).unwrap();
        s.state.apply(s.player_id, run, &s.catalog).unwrap();
        let id = driver(&s, &ui).unwrap().vehicle_id.unwrap();
        let stop = company(&s).unwrap().routes[0].stops[0];
        s.state.living.units.get_mut(&id).unwrap().position =
            s.state.transport.facilities[&stop].position;
        s.state.advance_transport(1.).unwrap();
        assert!(driver(&s, &ui).unwrap().trip.is_some());
        let hold = act(FleetAction::Hold, &mut ui, &mut s).unwrap();
        s.state.apply(s.player_id, hold, &s.catalog).unwrap();
        assert_eq!(driver(&s, &ui).unwrap().status, FleetStatus::Held);
        assert!(!unavailable(FleetAction::Run, &s, &ui));
        assert!(!unavailable(FleetAction::Return, &s, &ui));
        assert!(act(FleetAction::Return, &mut ui, &mut s).is_none());
        assert!(label(FleetLabel::Return, &s, &ui).contains("Confirm"));
        let returned = act(FleetAction::Return, &mut ui, &mut s).unwrap();
        s.state.apply(s.player_id, returned, &s.catalog).unwrap();
        assert!(label(FleetLabel::Driver, &s, &ui).contains("Return pickup"));
        assert!(unavailable(FleetAction::Return, &s, &ui));
        s.state.advance_transport(1.).unwrap();
        assert!(driver(&s, &ui).unwrap().trip.is_none());
        assert_eq!(company(&s).unwrap().revenue, 0);
    }

    #[test]
    fn depot_market_ui_uses_visible_quote_and_authority_rejects_a_changed_price() {
        let mut s = fixture();
        let mut ui = UiState::default();
        apply(
            &mut s,
            CompanyCommand::Create {
                name: "Market carrier".into(),
            },
        );
        apply(&mut s, CompanyCommand::Deposit { amount: 5000 });
        let facility = depot_facility(&s, &ui).unwrap().id;
        apply(
            &mut s,
            CompanyCommand::OpenDepot {
                facility_id: facility,
            },
        );
        let shop = *s.state.living.shops.keys().next().unwrap();
        s.state.living.shops.get_mut(&shop).unwrap().position =
            s.state.transport.facilities[&facility].position;
        ui.fleet.item = interface::TRADE_ITEMS
            .iter()
            .position(|(id, _)| *id == "wood")
            .unwrap();
        ui.fleet.quantity = 4;
        ui.fleet.market_shop = Some(shop);
        let quote = depot_market_quote(&s, &ui, true).unwrap();
        assert!(label(FleetLabel::DepotBuy, &s, &ui).contains(&money(quote.total_cost)));
        assert!(!unavailable(FleetAction::BuyDepotGoods, &s, &ui));
        ui.panel = Panel::Fleet;
        ui.fleet.tab = FleetTab::Depot;
        refresh_displayed_quotes(&s, &mut ui);
        let stale = act(FleetAction::BuyDepotGoods, &mut ui, &mut s).unwrap();
        s.state
            .living
            .shops
            .get_mut(&shop)
            .unwrap()
            .prices
            .get_mut("wood")
            .unwrap()
            .buy += 1;
        let before = s.state.clone();
        assert!(s.state.apply(s.player_id, stale, &s.catalog).is_err());
        assert_eq!(s.state, before);
        assert!(act(FleetAction::BuyDepotGoods, &mut ui, &mut s).is_none());
        assert!(s.last_message.contains("price or selection changed"));
        assert_eq!(s.state, before);
        refresh_displayed_quotes(&s, &mut ui);
        let personal = s.state.living.accounts[&s.player_id].money;
        let command = act(FleetAction::BuyDepotGoods, &mut ui, &mut s).unwrap();
        s.state.apply(s.player_id, command, &s.catalog).unwrap();
        assert_eq!(owned_depot(&s, &ui).unwrap().stock["wood"], 4);
        assert_eq!(s.state.living.accounts[&s.player_id].money, personal);
        assert!(!unavailable(FleetAction::SellDepotGoods, &s, &ui));
        s.state
            .set_player_position(s.player_id, s.state.living.origin.unwrap())
            .unwrap();
        if !at_depot_facility(&s, &ui) {
            assert!(unavailable(FleetAction::BuyDepotGoods, &s, &ui));
            assert!(act(FleetAction::BuyDepotGoods, &mut ui, &mut s).is_none());
        }
    }

    #[test]
    fn depot_route_shortcuts_create_distinct_owner_scoped_sale_and_transfer_contracts() {
        let mut s = fixture();
        let mut ui = UiState::default();
        apply(
            &mut s,
            CompanyCommand::Create {
                name: "Distribution carrier".into(),
            },
        );
        apply(&mut s, CompanyCommand::Deposit { amount: 10_000 });
        let source = depot_facility(&s, &ui).unwrap().id;
        let second = s
            .state
            .transport
            .facilities
            .values()
            .find(|f| f.kind == TransportFacilityKind::Depot)
            .unwrap()
            .id;
        let shop = *s.state.living.shops.keys().next().unwrap();
        let buyer = s
            .state
            .transport
            .facilities
            .values()
            .find(|f| f.linked_site == Some(shop))
            .unwrap()
            .id;
        let frame = terra_core::LocalFrame::new(s.state.living.origin.unwrap());
        for (id, x) in [(source, 500.), (second, 650.), (buyer, 800.)] {
            s.state.transport.facilities.get_mut(&id).unwrap().position =
                frame.to_geo([x, 0., -500.]).unwrap();
        }
        for id in [source, second] {
            s.state
                .set_player_position(s.player_id, s.state.transport.facilities[&id].position)
                .unwrap();
            apply(&mut s, CompanyCommand::OpenDepot { facility_id: id });
        }
        ui.fleet.depot = Some(source);
        s.state
            .set_player_position(s.player_id, s.state.transport.facilities[&source].position)
            .unwrap();
        ui.fleet.item = interface::TRADE_ITEMS
            .iter()
            .position(|(id, _)| *id == "wood")
            .unwrap();
        ui.fleet.quantity = 4;
        let deposit = act(FleetAction::DepositGoods, &mut ui, &mut s).unwrap();
        s.state.apply(s.player_id, deposit, &s.catalog).unwrap();
        for (action, expected) in [
            (FleetAction::SuggestDepotSale, FleetDraftKind::WarehouseSale),
            (
                FleetAction::SuggestDepotTransfer,
                FleetDraftKind::WarehouseTransfer,
            ),
        ] {
            assert!(
                act(action, &mut ui, &mut s).is_none(),
                "drafting spends no money"
            );
            assert!(ui.fleet.tab == FleetTab::Routes);
            assert!(ui.fleet.service_kind == expected);
            assert_eq!(ui.fleet.draft_stops[0], source);
            assert!(
                route_problem(&s, &ui).is_none(),
                "{}",
                label(FleetLabel::Draft, &s, &ui)
            );
            let command = act(FleetAction::SaveRoute, &mut ui, &mut s).unwrap();
            s.state.apply(s.player_id, command, &s.catalog).unwrap();
        }
        assert!(matches!(
            company(&s).unwrap().routes[0].service,
            FleetService::DepotFreight { .. }
        ));
        assert!(matches!(
            company(&s).unwrap().routes[1].service,
            FleetService::DepotTransfer { .. }
        ));
        assert_eq!(company(&s).unwrap().revenue, 0);
        s.state.validate_with_catalog(&s.catalog).unwrap();
    }
}
