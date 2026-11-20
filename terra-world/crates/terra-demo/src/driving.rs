//! Native transport presentation. All purchases, jobs, licenses and servicing
//! submit the same authoritative commands used by network clients.
use crate::{
    control::{ControlIntent, ControlMode, ControlQueue, ControlSet, PlayerControl},
    interface::{self, Action, Label, Panel, UiState},
};
use bevy::prelude::*;
use terra_bevy::rules::Command;
use terra_bevy::rules::*;
use terra_bevy::{
    ResidentChunks, SandboxIntent, SandboxQueue, SandboxRuntime, WorldRuntime, WorldSet,
    environment::physical_surface, render::ActorSceneOverrides,
};
use terra_core::{GeoPosition, LocalFrame};

#[derive(Default)]
pub struct DrivingUi {
    pub vehicle: Option<u64>,
    pub job: Option<u64>,
    pub garage: Option<u64>,
    pub model: usize,
    pub license: usize,
    pub course: Option<u64>,
    pub sell_confirm: bool,
    pub rental_return_confirm: bool,
    pub route_to_garage: bool,
    pub route_to_course: bool,
    pending_body: Option<(u64, f32)>,
    displayed_paint: Option<DisplayedPaint>,
    displayed_rental_return: Option<RentalReturnReview>,
    reviewed_rental_return: Option<RentalReturnReview>,
    displayed_rental_confirmation: bool,
}

/// Bind a queued paid click to the vehicle and appearance the Garage displayed.
/// This is local presentation state; the host's paint command stays unchanged.
#[derive(Clone, Copy, PartialEq, Eq)]
struct DisplayedPaint {
    player_id: u64,
    restore_revision: u64,
    vehicle_id: u64,
    model: VehicleModel,
    paint: u8,
    garage_id: u64,
}

/// A local review of the exact rental and refund shown in Garage. The running
/// prepaid clock is deliberately excluded: time alone does not change a refund.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RentalReturnReview {
    player_id: u64,
    restore_revision: u64,
    vehicle_id: u64,
    model: VehicleModel,
    garage_id: u64,
    rental_garage_id: u64,
    bond_escrow: u64,
    quote: RentalReturnQuote,
}

/// An interaction key targets the active vehicle's accepted stop, independently
/// of whichever offer the player last browsed. Retain that intent while queued.
#[derive(Clone, Copy)]
pub struct ActiveStop {
    player_id: u64,
    restore_revision: u64,
    vehicle_id: u64,
    job_id: u64,
    phase: TransportJobPhase,
    deadline: f64,
}
#[derive(Clone, Copy)]
pub enum DrivingAction {
    NextVehicle,
    Enter,
    Exit,
    NextJob,
    Accept,
    Service,
    ServiceActive(ActiveStop),
    Cancel,
    Autopilot,
    NextGarage,
    RouteGarage,
    NextModel,
    PreviousModel,
    Buy,
    Rent,
    RenewRental,
    ReturnRental,
    Sell,
    Refuel,
    Repair,
    Paint,
    UpgradeEngine,
    UpgradeBrakes,
    NextLicense,
    License,
    Lights,
    LeftSignal,
    RightSignal,
    Hazards,
    ParkingBrake,
    BuyTrailer,
    Hitch,
    Unhitch,
    NextCourse,
    RouteCourse,
    StartRace,
    AbandonRace,
}
#[derive(Clone, Copy)]
pub enum DrivingLabel {
    Vehicle,
    Driver,
    Job,
    Service,
    Garage,
    Model,
    License,
    Race,
    Sell,
    Fuel,
    Refuel,
    Repair,
    Paint,
    Upgrade,
    Signals,
    RentalOffer,
    Rental,
    ReturnRental,
}

pub struct DrivingPlugin;
#[derive(Component)]
struct DrivingHud;
#[derive(Component)]
struct SpeedText;
#[derive(Component)]
struct NavigationText;

impl Plugin for DrivingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NavigationPath>()
            .add_systems(Startup, (setup_hud, setup_route_map))
            .add_systems(
                Update,
                (complete_body_transition, driving_keys)
                    .chain()
                    .before(ControlSet::Input),
            )
            .add_systems(
                Update,
                (
                    update_navigation_path,
                    hud,
                    navigation,
                    race_gates,
                    route_map,
                )
                    .chain()
                    .after(WorldSet::Sandbox),
            );
    }
}

fn line(p: &mut ChildSpawnerCommands, label: DrivingLabel, prominent: bool) {
    p.spawn((
        Label::Driving(label),
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
fn button(p: &mut ChildSpawnerCommands, title: &str, action: DrivingAction) {
    interface::button(p, title, Action::Driving(action));
}
pub fn panels(p: &mut ChildSpawnerCommands) {
    use DrivingAction as A;
    interface::page(p, Panel::Driving, |p| {
        line(p, DrivingLabel::Driver, false);
        interface::heading(p, "Your vehicle");
        line(p, DrivingLabel::Vehicle, true);
        button(p, "Choose next owned vehicle", A::NextVehicle);
        interface::pair(
            p,
            ("Enter vehicle", Action::Driving(A::Enter)),
            ("Exit on foot", Action::Driving(A::Exit)),
        );
        interface::heading(p, "Transport work");
        line(p, DrivingLabel::Job, true);
        button(p, "Browse next available job", A::NextJob);
        button(p, "Accept with selected vehicle", A::Accept);
        interface::button_with_label(
            p,
            "",
            Action::Driving(A::Service),
            Some(Label::Driving(DrivingLabel::Service)),
        );
        button(p, "Autopilot to the marked stop", A::Autopilot);
        button(p, "Return or cancel this job", A::Cancel);
        p.spawn(interface::text("Drive to each stop and come to rest. Freight is taken from real stocks; passengers and rewards are reserved when you accept. A loaded job must be returned to its pickup to cancel.",12.,interface::MUTED));
        interface::heading(p, "Vehicle controls");
        line(p, DrivingLabel::Signals, false);
        interface::pair(
            p,
            ("Headlights", Action::Driving(A::Lights)),
            ("Park brake", Action::Driving(A::ParkingBrake)),
        );
        interface::pair(
            p,
            ("Left signal", Action::Driving(A::LeftSignal)),
            ("Right signal", Action::Driving(A::RightSignal)),
        );
        button(p, "Hazard lights", A::Hazards);
        interface::heading(p, "Time trials");
        line(p, DrivingLabel::Race, false);
        button(p, "Choose course", A::NextCourse);
        button(p, "Mark course start", A::RouteCourse);
        interface::pair(
            p,
            ("Start time trial", Action::Driving(A::StartRace)),
            ("Abandon run", Action::Driving(A::AbandonRace)),
        );
        p.spawn(interface::text("Gates register automatically in order. Vehicle distance and physical elapsed time are checked by the host.",12.,interface::MUTED));
        button(p, "Open garage", A::RouteGarage);
    });
    interface::page(p, Panel::Garage, |p| {
        line(p, DrivingLabel::Garage, true);
        button(p, "Choose next garage", A::NextGarage);
        button(p, "Mark garage on the world", A::RouteGarage);
        line(p, DrivingLabel::Vehicle, false);
        button(p, "Choose next owned vehicle", A::NextVehicle);
        interface::heading(p, "Service and customization");
        line(p, DrivingLabel::Fuel, false);
        interface::button_with_label(
            p,
            "",
            Action::Driving(A::Refuel),
            Some(Label::Driving(DrivingLabel::Refuel)),
        );
        line(p, DrivingLabel::Repair, false);
        button(p, "Repair vehicle", A::Repair);
        line(p, DrivingLabel::Paint, false);
        button(p, "Apply next paint · 5 cr", A::Paint);
        line(p, DrivingLabel::Upgrade, false);
        interface::pair(
            p,
            ("Upgrade engine", Action::Driving(A::UpgradeEngine)),
            ("Upgrade brakes", Action::Driving(A::UpgradeBrakes)),
        );
        interface::heading(p, "Vehicle showroom");
        line(p, DrivingLabel::Model, true);
        interface::pair(
            p,
            ("Previous model", Action::Driving(A::PreviousModel)),
            ("Next model", Action::Driving(A::NextModel)),
        );
        button(p, "Purchase selected model", A::Buy);
        interface::heading(p, "Rent before you buy");
        line(p, DrivingLabel::RentalOffer, false);
        button(p, "Rent selected model · five minutes", A::Rent);
        button(p, "Choose owned or rented vehicle", A::NextVehicle);
        line(p, DrivingLabel::Rental, false);
        button(p, "Renew selected rental · five minutes", A::RenewRental);
        interface::button_with_label(
            p,
            "Return empty rental",
            Action::Driving(A::ReturnRental),
            Some(Label::Driving(DrivingLabel::ReturnRental)),
        );
        p.spawn(interface::text("Rental time uses physical seconds and freezes offline. Expiry stops motion and preserves every load. Renew from anywhere, then drive back to the original garage to return. Exit, release company work and unhitch first. Unused time is not refunded; remaining fuel and the undamaged bond are.",12.,interface::MUTED));
        interface::button_with_label(
            p,
            "",
            Action::Driving(A::Sell),
            Some(Label::Driving(DrivingLabel::Sell)),
        );
        interface::heading(p, "Driver qualifications");
        line(p, DrivingLabel::License, false);
        button(p, "Choose qualification", A::NextLicense);
        button(p, "Obtain qualification", A::License);
        interface::heading(p, "Cargo trailers");
        button(p, "Buy cargo trailer · 150 cr", A::BuyTrailer);
        interface::pair(
            p,
            ("Hitch nearby trailer", Action::Driving(A::Hitch)),
            ("Detach trailer", Action::Driving(A::Unhitch)),
        );
        p.spawn(interface::text("Service requires you and the stopped vehicle within 20 m of the garage. Park, exit and unload a vehicle before selling. Trailers add freight capacity and follow their towing vehicle.",12.,interface::MUTED));
    });
}

fn credits(n: u64) -> String {
    format!("{:.2}", n as f64 / 100.)
}
pub fn vehicle<'a>(s: &'a SandboxRuntime, ui: &UiState) -> Option<&'a VehicleState> {
    let world = &s.state.transport;
    ui.driving
        .vehicle
        .and_then(|id| world.vehicles.get(&id))
        .filter(|v| v.owner == s.player_id)
        .or_else(|| {
            s.state
                .living
                .active_unit(s.player_id)
                .and_then(|u| world.vehicles.get(&u.id))
                .filter(|v| v.owner == s.player_id)
        })
        .or_else(|| world.vehicles.values().find(|v| v.owner == s.player_id))
}
pub fn job<'a>(s: &'a SandboxRuntime, ui: &UiState) -> Option<&'a TransportJob> {
    let visible = |j: &&TransportJob| j.assignment.as_ref().is_none_or(|a| a.owner == s.player_id);
    ui.driving
        .job
        .and_then(|id| s.state.transport.jobs.get(&id))
        .filter(|j| visible(j))
        .or_else(|| {
            s.state.transport.jobs.values().find(|j| {
                j.assignment
                    .as_ref()
                    .is_some_and(|a| a.owner == s.player_id)
            })
        })
        .or_else(|| s.state.transport.jobs.values().find(visible))
}
fn garage<'a>(s: &'a SandboxRuntime, ui: &UiState) -> Option<&'a TransportFacility> {
    ui.driving
        .garage
        .and_then(|id| s.state.transport.facilities.get(&id))
        .filter(|f| f.kind == TransportFacilityKind::Garage)
        .or_else(|| {
            s.state
                .transport
                .facilities
                .values()
                .filter(|f| f.kind == TransportFacilityKind::Garage)
                .min_by(|a, b| distance_to(s, a.position).total_cmp(&distance_to(s, b.position)))
        })
}

fn fixed_vehicle_livery(
    s: &SandboxRuntime,
    ui: &UiState,
    scenes: Option<&ActorSceneOverrides>,
) -> bool {
    let Some(scenes) = scenes else {
        return false;
    };
    let Some(vehicle) = vehicle(s, ui) else {
        return false;
    };
    let Some(body) = s.state.living.units.get(&vehicle.unit_id) else {
        return false;
    };
    // Match the renderer's actual kind, public catalog model and species lookup.
    // A registered scene stays fixed-livery while loading or using fallback.
    scenes
        .resolve_model(
            body.kind,
            s.state
                .ecology
                .animals
                .get(&body.id)
                .map(|animal| animal.species),
            s.state
                .transport
                .appearance(body.id)
                .map(|appearance| appearance.model),
        )
        .is_some()
}

fn paint_context(
    s: &SandboxRuntime,
    ui: &UiState,
    scenes: Option<&ActorSceneOverrides>,
) -> Option<DisplayedPaint> {
    if !ui.context_panel || ui.panel != Panel::Garage || fixed_vehicle_livery(s, ui, scenes) {
        return None;
    }
    let vehicle = vehicle(s, ui)?;
    Some(DisplayedPaint {
        player_id: s.player_id,
        restore_revision: s.restore_revision,
        vehicle_id: vehicle.unit_id,
        model: vehicle.model,
        paint: vehicle.paint,
        garage_id: garage(s, ui)?.id,
    })
}

fn rental_return_context(s: &SandboxRuntime, ui: &UiState) -> Option<RentalReturnReview> {
    if !ui.context_panel || ui.panel != Panel::Garage {
        return None;
    }
    let vehicle = vehicle(s, ui)?;
    let rental = vehicle.rental.as_ref()?;
    Some(RentalReturnReview {
        player_id: s.player_id,
        restore_revision: s.restore_revision,
        vehicle_id: vehicle.unit_id,
        model: vehicle.model,
        garage_id: garage(s, ui)?.id,
        rental_garage_id: rental.facility_id,
        bond_escrow: rental.bond_escrow,
        quote: s
            .state
            .rental_return_quote(s.player_id, vehicle.unit_id)
            .ok()?,
    })
}

pub fn refresh_displayed(
    s: &SandboxRuntime,
    ui: &mut UiState,
    scenes: Option<&ActorSceneOverrides>,
) {
    ui.driving.displayed_paint = paint_context(s, ui, scenes);
    let rental_return = rental_return_context(s, ui);
    if ui.driving.rental_return_confirm && ui.driving.reviewed_rental_return != rental_return {
        ui.driving.rental_return_confirm = false;
        ui.driving.reviewed_rental_return = None;
    }
    ui.driving.displayed_rental_return = rental_return;
    ui.driving.displayed_rental_confirmation =
        rental_return.is_some() && ui.driving.rental_return_confirm;
}

fn course<'a>(s: &'a SandboxRuntime, ui: &UiState) -> Option<&'a RaceCourse> {
    ui.driving
        .course
        .and_then(|id| s.state.transport.courses.get(&id))
        .or_else(|| s.state.transport.courses.values().next())
}
fn license(ui: &UiState) -> DriverLicense {
    [
        DriverLicense::Passenger,
        DriverLicense::Heavy,
        DriverLicense::Emergency,
    ][ui.driving.license % 3]
}
fn distance_to(s: &SandboxRuntime, p: GeoPosition) -> f64 {
    s.state
        .players
        .get(&s.player_id)
        .map_or(f64::INFINITY, |u| {
            u.position
                .surface_distance_m(p)
                .hypot(u.position.altitude_m - p.altitude_m)
        })
}
fn speed(s: &SandboxRuntime, id: u64) -> f64 {
    s.state.living.units.get(&id).map_or(f64::INFINITY, |u| {
        u.velocity_enu_m_s[0].hypot(u.velocity_enu_m_s[1])
    })
}
fn within(a: GeoPosition, b: GeoPosition, metres: f64) -> bool {
    a.surface_distance_m(b).hypot(a.altitude_m - b.altitude_m) <= metres
}
fn active_vehicle(s: &SandboxRuntime) -> Option<&VehicleState> {
    let account = s.state.living.accounts.get(&s.player_id)?;
    account
        .possessed
        .then(|| s.state.transport.vehicles.get(&account.active_unit))
        .flatten()
        .filter(|v| v.owner == s.player_id)
}
pub fn driver_body(s: &SandboxRuntime) -> Option<&LivingUnit> {
    let account = s.state.living.accounts.get(&s.player_id)?;
    s.state.living.units.get(&account.avatar_unit)
}
fn refuel_quantity(tank_space: f64, station_stock: f64, money: u64) -> u16 {
    if !tank_space.is_finite() || !station_stock.is_finite() {
        return 0;
    }
    tank_space
        .floor()
        .clamp(0., 10.)
        .min(station_stock.floor().max(0.))
        .min((money / 150) as f64) as u16
}
fn fuel_offer(s: &SandboxRuntime, ui: &UiState) -> u16 {
    let Some(v) = vehicle(s, ui) else {
        return 0;
    };
    let Some(g) = garage(s, ui) else {
        return 0;
    };
    refuel_quantity(
        v.model.spec().fuel_capacity_l - v.fuel_l,
        g.fuel_l,
        s.state
            .living
            .accounts
            .get(&s.player_id)
            .map_or(0, |a| a.money),
    )
}
fn licensed(s: &SandboxRuntime, v: &VehicleState) -> bool {
    s.state
        .transport
        .drivers
        .get(&s.player_id)
        .is_some_and(|d| d.licenses.contains(&v.model.spec().required_license))
}
fn assigned(s: &SandboxRuntime, id: u64) -> bool {
    s.state.is_company_vehicle(id)
        || s.state
            .transport
            .jobs
            .values()
            .any(|j| j.assignment.as_ref().is_some_and(|a| a.vehicle_id == id))
}
fn job_blocker(
    s: &SandboxRuntime,
    j: &TransportJob,
    v: Option<&VehicleState>,
) -> Option<&'static str> {
    if j.assignment.is_some() {
        return None;
    }
    let Some(v) = v else {
        return Some("Choose an owned vehicle first.");
    };
    if !licensed(s, v) {
        return Some("Obtain this vehicle's driver qualification at a garage.");
    }
    if v.rental.as_ref().is_some_and(|r| r.remaining_seconds <= 0.) {
        return Some("Renew this rental in Garage before accepting new work.");
    }
    if !j.kind.accepts(v.model) {
        return Some("Choose a vehicle suitable for this type of work.");
    }
    if assigned(s, v.unit_id)
        || !v.cargo.is_empty()
        || s.state
            .transport
            .races
            .values()
            .any(|r| r.vehicle_id == v.unit_id)
    {
        return Some("Finish this vehicle's work or time trial and unload it first.");
    }
    if s.state
        .transport
        .jobs
        .values()
        .filter(|j| {
            j.assignment
                .as_ref()
                .is_some_and(|a| a.owner == s.player_id)
        })
        .count()
        >= MAX_PLAYER_JOBS
    {
        return Some("Finish one of your accepted jobs first.");
    }
    if j.passengers > v.model.spec().passenger_seats
        || j.amount > s.state.vehicle_cargo_capacity(v.unit_id)
    {
        return Some("Choose more seats or cargo capacity; a trailer can carry freight.");
    }
    None
}
fn next(ids: Vec<u64>, current: Option<u64>) -> Option<u64> {
    if ids.is_empty() {
        None
    } else {
        Some(
            ids[current
                .and_then(|id| ids.iter().position(|x| *x == id))
                .map_or(0, |i| (i + 1) % ids.len())],
        )
    }
}
fn target<'a>(s: &'a SandboxRuntime, ui: &UiState) -> Option<(&'a str, GeoPosition)> {
    if let Some(run) = s.state.transport.races.get(&s.player_id) {
        if let Some(c) = s.state.transport.courses.get(&run.course_id) {
            return c
                .checkpoints
                .get(run.next_checkpoint)
                .map(|p| (c.name.as_str(), *p));
        }
    }
    if ui.driving.route_to_garage {
        return garage(s, ui).map(|f| (f.name.as_str(), f.position));
    }
    if ui.driving.route_to_course {
        return course(s, ui).and_then(|c| c.checkpoints.first().map(|p| (c.name.as_str(), *p)));
    }
    job_target(s, ui)
}
fn job_target<'a>(s: &'a SandboxRuntime, ui: &UiState) -> Option<(&'a str, GeoPosition)> {
    let j = job(s, ui)?;
    let id = if j
        .assignment
        .as_ref()
        .is_some_and(|a| a.phase == TransportJobPhase::ToDestination)
    {
        j.destination_facility
    } else {
        j.pickup_facility
    };
    s.state
        .transport
        .facilities
        .get(&id)
        .map(|f| (f.name.as_str(), f.position))
}

pub fn handle_vehicle_interaction(
    s: &mut SandboxRuntime,
    ui: &mut UiState,
    queue: &mut interface::UiQueue,
) -> bool {
    let Some(account) = s.state.living.accounts.get(&s.player_id) else {
        return false;
    };
    let vehicle_id = account.active_unit;
    if !s.state.transport.vehicles.contains_key(&vehicle_id) {
        return false;
    }
    ui.driving.vehicle = Some(vehicle_id);
    ui.panel = Panel::Driving;
    ui.context_panel = true;
    ui.panel_navigation_open = false;
    let Some(job) = s.state.transport.jobs.values().find(|job| {
        job.assignment
            .as_ref()
            .is_some_and(|a| a.owner == s.player_id && a.vehicle_id == vehicle_id)
    }) else {
        s.last_message = "This vehicle has no accepted transport job. Review an offer in Driving, then accept it with this vehicle.".into();
        return true;
    };
    let assignment = job.assignment.as_ref().unwrap();
    let stop = ActiveStop {
        player_id: s.player_id,
        restore_revision: s.restore_revision,
        vehicle_id,
        job_id: job.id,
        phase: assignment.phase,
        deadline: assignment.deadline,
    };
    ui.driving.job = Some(job.id);
    ui.driving.route_to_garage = false;
    ui.driving.route_to_course = false;
    if speed(s, vehicle_id) > 0.5 {
        s.last_message =
            "Stop the vehicle before collecting, delivering or returning its load.".into();
    } else if let Some((name, position)) = job_target(s, ui)
        && distance_to(s, position) > 20.
    {
        s.last_message = format!(
            "Your accepted stop is {name}, {:.0} m away. Drive there and stop within 20 m to use it.",
            distance_to(s, position)
        );
    } else if unavailable(DrivingAction::Service, s, ui, None) {
        s.last_message =
            "The accepted transport stop is unavailable in the current view. Review Driving."
                .into();
    } else {
        queue
            .0
            .push_back(Action::Driving(DrivingAction::ServiceActive(stop)));
    }
    true
}

fn vehicle_label(s: &SandboxRuntime, v: Option<&VehicleState>) -> String {
    let Some(v) = v else {
        return "Your transport fleet will appear when the settlement is ready.".into();
    };
    let mut label = format!(
        "{} · #{}\nFuel {:.1}/{:.0} L · condition {:.0}%\n{:.1} km · {} cargo units{}",
        v.model.spec().name,
        v.unit_id,
        v.fuel_l,
        v.model.spec().fuel_capacity_l,
        v.condition * 100.,
        v.odometer_m / 1000.,
        v.cargo.values().sum::<u32>(),
        if v.trailer.is_some() {
            " · trailer"
        } else {
            ""
        }
    );
    if let Some(rental) = &v.rental {
        if rental.remaining_seconds <= 0. {
            label.push_str("\nRental expired · renew in Garage to drive");
        } else {
            label.push_str(&format!(
                "\nRental: {:.0} s prepaid",
                rental.remaining_seconds
            ));
        }
    }
    if s.state.is_company_vehicle(v.unit_id) {
        label.push_str("\nAssigned to your company · manage it in Fleet");
    } else if !licensed(s, v) {
        label.push_str(&format!(
            "\nRequires {} qualification",
            v.model.spec().required_license.name()
        ));
    } else if let Some(unit) = s.state.living.units.get(&v.unit_id) {
        label.push_str(&format!(
            "\n{:.0} m away · stop within 20 m to enter or service",
            distance_to(s, unit.position)
        ));
    }
    label
}

fn service_label(
    s: &SandboxRuntime,
    ui: &UiState,
    input_settings: &crate::player_input::InputSettings,
) -> String {
    let Some(assignment) = job(s, ui).and_then(|j| j.assignment.as_ref()) else {
        return "Accept a job to use its stops".into();
    };
    let action = match assignment.phase {
        TransportJobPhase::ToPickup => "Collect cargo / board passengers",
        TransportJobPhase::ToDestination => "Deliver and receive payment",
        TransportJobPhase::Returning => "Return cargo / passengers",
    };
    if s.state
        .living
        .accounts
        .get(&s.player_id)
        .is_some_and(|a| a.active_unit == assignment.vehicle_id)
    {
        format!(
            "{action} [{}]",
            input_settings.key_label(crate::player_input::Binding::Interact)
        )
    } else {
        action.into()
    }
}

pub fn label(
    which: DrivingLabel,
    s: &SandboxRuntime,
    ui: &UiState,
    scenes: Option<&ActorSceneOverrides>,
    input_settings: &crate::player_input::InputSettings,
) -> String {
    use DrivingLabel as L;
    let v = vehicle(s, ui);
    let world = &s.state.transport;
    let progress = world.drivers.get(&s.player_id);
    match which {
        L::Vehicle => vehicle_label(s,v),
        L::Driver=>progress.map(|p|format!("Driver level {} · {} jobs completed\n{} cr earned · {:.1} km driven",p.level(),p.jobs_completed,credits(p.earnings),p.distance_m/1000.)).unwrap_or_else(||"Preparing driver services…".into()),
        L::Job=>job(s,ui).map(|j| {
            let place=|id|world.facilities.get(&id).map_or("Regional stop",|f|f.name.as_str());
            let phase=j.assignment.as_ref().map_or("Available",|a|match a.phase {TransportJobPhase::ToPickup=>"Collect at pickup",TransportJobPhase::ToDestination=>"Deliver to destination",TransportJobPhase::Returning=>"Return to pickup"});
            let cargo=j.item.as_ref().map(|i|format!("{} {}",j.amount,i.replace('_'," "))).unwrap_or_else(||format!("{} passengers",j.passengers));
            let time=j.assignment.as_ref().map(|a|format!("\n{:.0} s remaining",(a.deadline-world.elapsed_seconds).max(0.))).unwrap_or_default();
            let hint=job_blocker(s,j,v).map(|reason|format!("\n{reason}")).unwrap_or_default();
            format!("{}\n{} · {} cr\n{} → {}\n{} · {}{}{}",j.name,j.kind.name(),credits(j.reward),place(j.pickup_facility),place(j.destination_facility),phase,cargo,time,hint)
        }).unwrap_or_else(||"No transport offers in this region.".into()),
        L::Service=>service_label(s, ui, input_settings),
        L::Garage=>garage(s,ui).map(|g|format!("{}\n{:.0} m from active actor\nStation supply {:.0} L",g.name,distance_to(s,g.position),g.fuel_l)).unwrap_or_else(||"No garage loaded.".into()),
        L::Model=>{let m=VEHICLE_MODELS[ui.driving.model%VEHICLE_MODELS.len()];let x=m.spec(); format!("{} · {} cr\n{:.0} km/h · {} passenger seats\n{} cargo capacity · {} license",x.name,credits(x.price),x.max_speed_m_s*3.6,x.passenger_seats,x.cargo_capacity,x.required_license.name())},
        L::License=>{let l=license(ui);format!("{} · {} cr\nRequires {} completed jobs · {}",l.name(),credits(l.cost()),l.required_jobs(),if progress.is_some_and(|p|p.licenses.contains(&l)){"already held"}else{"not held"})},
        L::Race=>course(s,ui).map(|c|{let best=world.personal_bests.iter().filter(|b|b.player_id==s.player_id&&b.course_id==c.id).map(|b|b.seconds).min_by(f64::total_cmp);let run=world.races.get(&s.player_id);format!("{} · {} gates\n{}\n{}",c.name,c.checkpoints.len(),run.map(|r|format!("Gate {} · {:.1} s",r.next_checkpoint+1,world.elapsed_seconds-r.started_at)).unwrap_or_else(||format!("Start {:.0} m away · enter an unassigned vehicle\nMark the start, then drive within 20 m",c.checkpoints.first().map_or(f64::INFINITY,|p|distance_to(s,*p)))),best.map(|t|format!("Personal best {t:.2} s")).unwrap_or_else(||"Set your first time".into()))}).unwrap_or_else(||"No time-trial course loaded.".into()),
        L::Sell=>v.filter(|v|v.rental.is_none()).map(|v|format!("{} · {} cr",if ui.driving.sell_confirm{"Confirm vehicle sale"}else{"Sell selected vehicle"},credits((v.model.spec().price as f64*0.65*f64::from(v.condition)).round() as u64))).unwrap_or_else(||"Select a vehicle to sell".into()),
        L::Fuel=>v.map(|v|format!("{:.1} L / {:.0} L · 1.50 cr per litre",v.fuel_l,v.model.spec().fuel_capacity_l)).unwrap_or_default(),
        L::Refuel=>{let n=fuel_offer(s,ui);if n>0{format!("Buy {n} L · {} cr",credits(u64::from(n)*150))}else{"Fuel unavailable · check tank, funds and station stock".into()}},
        L::Repair=>v.map(|v|format!("Repair estimate {} cr",credits(repair_price(v)))).unwrap_or_default(),
        L::Paint=>if fixed_vehicle_livery(s,ui,scenes){"The active model has a fixed livery. Paint is unavailable.".into()}else{"Paint changes the selected vehicle's exterior.".into()},
        L::Upgrade=>v.map(|v|format!("Engine {}/3 · next {} cr\nBrakes {}/3 · next {} cr",v.engine_level,credits(upgrade_price(v.engine_level)),v.brakes_level,credits(upgrade_price(v.brakes_level)))).unwrap_or_default(),
        L::RentalOffer=> { let model=VEHICLE_MODELS[ui.driving.model%VEHICLE_MODELS.len()]; let offer=rental_offer(model);format!("{}\n5-minute fee: {} cr\nSecurity bond: {} cr\n10 L fuel: {} cr\nDue now: {} personal cr",model.spec().name,credits(offer.fee),credits(offer.bond),credits(offer.fuel_cost),credits(offer.total)) },
        L::Rental=>v.and_then(|v|v.rental.as_ref().map(|r|(v,r))).map(|(v,r)| {
            let quote=s.state.rental_return_quote(s.player_id,v.unit_id).ok();
            format!("Selected rental: {}\n{} · renewal {} cr\nReturn garage: {}\n{}",v.model.spec().name,if r.remaining_seconds>0.{format!("{:.0} s prepaid",r.remaining_seconds)}else{"Expired · renew to drive".into()},credits(rental_offer(v.model).fee),world.facilities.get(&r.facility_id).map(|f|f.name.as_str()).unwrap_or("Original garage"),quote.map(|q|format!("Return refund: {} cr\nBond refund {} · fuel {} · damage withheld {}",credits(q.total_refund),credits(q.bond_refund),credits(q.fuel_refund),credits(q.damage_cost))).unwrap_or_default())
        }).unwrap_or_else(||"Select a rented vehicle to review its time and return refund.".into()),
        L::ReturnRental=> if ui.driving.rental_return_confirm {"Confirm rental return and refund".into()}else{"Return empty rental".into()},
        L::Signals=>active_vehicle(s).map(|v|format!("Controlling {} #{}\nLights {} · {:?}\nParking brake {} · autopilot {}",v.model.spec().name,v.unit_id,if v.lights{"on"}else{"off"},v.indicator,if v.parking_brake{"engaged"}else{"released"},if v.autopilot{"on"}else{"off"})).unwrap_or_else(||"Enter a vehicle to use its controls.".into()),
    }
}

pub fn act(
    action: DrivingAction,
    ui: &mut UiState,
    s: &mut SandboxRuntime,
    control: &mut PlayerControl,
    queue: &mut ControlQueue,
    scenes: Option<&ActorSceneOverrides>,
) -> Option<Command> {
    use DrivingAction as A;
    let v = vehicle(s, ui).cloned();
    let j = job(s, ui).cloned();
    let g = garage(s, ui).map(|g| g.id);
    let wrap = |action| Some(Command::Transport { action });
    if !matches!(action, A::ReturnRental) {
        ui.driving.rental_return_confirm = false;
        ui.driving.reviewed_rental_return = None;
        ui.driving.displayed_rental_confirmation = false;
    }
    if !matches!(action, A::Sell) {
        ui.driving.sell_confirm = false;
    }
    match action {
        A::NextVehicle => {
            ui.driving.vehicle = next(
                s.state
                    .transport
                    .vehicles
                    .values()
                    .filter(|v| v.owner == s.player_id)
                    .map(|v| v.unit_id)
                    .collect(),
                v.map(|v| v.unit_id),
            );
            None
        }
        A::Enter => {
            let id = v?.unit_id;
            ui.driving.pending_body = Some((id, 0.));
            wrap(TransportCommand::EnterVehicle { vehicle_id: id })
        }
        A::Exit => {
            let avatar = s.state.living.accounts.get(&s.player_id)?.avatar_unit;
            // Once authority switches to the avatar, implicit selection would
            // fall back to the first owned vehicle. Keep the displayed choice,
            // including an explicitly selected vehicle other than the driver.
            ui.driving.vehicle = v.map(|v| v.unit_id);
            ui.driving.pending_body = Some((avatar, 0.));
            wrap(TransportCommand::ExitVehicle)
        }
        A::NextJob => {
            ui.driving.job = next(
                s.state
                    .transport
                    .jobs
                    .values()
                    .filter(|j| j.assignment.as_ref().is_none_or(|a| a.owner == s.player_id))
                    .map(|j| j.id)
                    .collect(),
                j.map(|j| j.id),
            );
            ui.driving.route_to_garage = false;
            ui.driving.route_to_course = false;
            None
        }
        A::Accept => {
            ui.driving.route_to_garage = false;
            ui.driving.route_to_course = false;
            wrap(TransportCommand::AcceptJob {
                job_id: j?.id,
                vehicle_id: v?.unit_id,
            })
        }
        A::Service => {
            let j = j?;
            wrap(match j.assignment?.phase {
                TransportJobPhase::ToPickup => TransportCommand::PickupJob { job_id: j.id },
                TransportJobPhase::ToDestination => TransportCommand::CompleteJob { job_id: j.id },
                TransportJobPhase::Returning => TransportCommand::CancelJob { job_id: j.id },
            })
        }
        A::ServiceActive(stop) => {
            let current = s.state.transport.jobs.get(&stop.job_id);
            if stop.player_id != s.player_id
                || stop.restore_revision != s.restore_revision
                || s.state
                    .living
                    .accounts
                    .get(&s.player_id)
                    .is_none_or(|a| a.active_unit != stop.vehicle_id)
                || current.and_then(|j| j.assignment.as_ref()).is_none_or(|a| {
                    a.owner != stop.player_id
                        || a.vehicle_id != stop.vehicle_id
                        || a.phase != stop.phase
                        || a.deadline != stop.deadline
                })
            {
                s.last_message =
                    "Your active transport stop changed. Review Driving before using it again."
                        .into();
                return None;
            }
            wrap(match stop.phase {
                TransportJobPhase::ToPickup => TransportCommand::PickupJob {
                    job_id: stop.job_id,
                },
                TransportJobPhase::ToDestination => TransportCommand::CompleteJob {
                    job_id: stop.job_id,
                },
                TransportJobPhase::Returning => TransportCommand::CancelJob {
                    job_id: stop.job_id,
                },
            })
        }
        A::Cancel => wrap(TransportCommand::CancelJob { job_id: j?.id }),
        A::Autopilot => {
            let v = v?;
            let (_, destination) = target(s, ui)?;
            let position = s.state.living.units.get(&v.unit_id)?.position;
            let frame = LocalFrame::new(destination);
            let d = frame.to_local(position);
            let n = d[0].hypot(d[2]);
            let approach = frame
                .to_geo(if n > 0.1 {
                    [d[0] / n * 12., 0., d[2] / n * 12.]
                } else {
                    [12., 0., 0.]
                })
                .ok()?;
            control.selected.clear();
            control.selected.insert(v.unit_id);
            queue.0.push_back(ControlIntent::Release);
            wrap(TransportCommand::SetAutopilot {
                vehicle_id: v.unit_id,
                enabled: true,
                destination: Some(approach),
            })
        }
        A::NextGarage => {
            ui.driving.garage = next(
                s.state
                    .transport
                    .facilities
                    .values()
                    .filter(|f| f.kind == TransportFacilityKind::Garage)
                    .map(|f| f.id)
                    .collect(),
                g,
            );
            None
        }
        A::RouteGarage => {
            ui.driving.route_to_garage = true;
            ui.driving.route_to_course = false;
            ui.panel = Panel::Garage;
            ui.context_panel = true;
            s.last_message =
                "Garage marked. Drive there, or use the marked-stop autopilot in Driving.".into();
            None
        }
        A::NextModel => {
            ui.driving.model = (ui.driving.model + 1) % VEHICLE_MODELS.len();
            None
        }
        A::PreviousModel => {
            ui.driving.model = (ui.driving.model + VEHICLE_MODELS.len() - 1) % VEHICLE_MODELS.len();
            None
        }
        A::Buy => wrap(TransportCommand::BuyVehicle {
            model: VEHICLE_MODELS[ui.driving.model % VEHICLE_MODELS.len()],
            facility_id: g?,
        }),
        A::Rent => wrap(TransportCommand::RentVehicle {
            model: VEHICLE_MODELS[ui.driving.model % VEHICLE_MODELS.len()],
            facility_id: g?,
        }),
        A::RenewRental => wrap(TransportCommand::RenewRental {
            vehicle_id: v?.unit_id,
        }),
        A::ReturnRental => {
            let displayed = ui.driving.displayed_rental_return;
            if displayed.is_none() || displayed != rental_return_context(s, ui) {
                ui.driving.rental_return_confirm = false;
                ui.driving.reviewed_rental_return = None;
                ui.driving.displayed_rental_return = None;
                ui.driving.displayed_rental_confirmation = false;
                s.last_message = "The rental or refund changed. Review the current vehicle, garage and refund before returning it.".into();
                return None;
            }
            if unavailable(A::ReturnRental, s, ui, scenes) {
                ui.driving.rental_return_confirm = false;
                ui.driving.reviewed_rental_return = None;
                ui.driving.displayed_rental_confirmation = false;
                s.last_message = "Return requires you and the stopped, empty rental at its original garage. Exit, finish its work and unhitch first.".into();
                return None;
            }
            if !ui.driving.rental_return_confirm {
                ui.driving.rental_return_confirm = true;
                ui.driving.reviewed_rental_return = displayed;
                ui.driving.displayed_rental_confirmation = false;
                s.last_message="Review the damage, bond and fuel refund, then confirm the empty rental return.".into();
                None
            } else if !ui.driving.displayed_rental_confirmation
                || ui.driving.reviewed_rental_return != displayed
            {
                s.last_message =
                    "Review the displayed rental refund before confirming its return.".into();
                None
            } else {
                let reviewed = ui.driving.reviewed_rental_return.take()?;
                ui.driving.rental_return_confirm = false;
                ui.driving.displayed_rental_return = None;
                ui.driving.displayed_rental_confirmation = false;
                wrap(TransportCommand::ReturnRental {
                    vehicle_id: reviewed.vehicle_id,
                    facility_id: reviewed.garage_id,
                    quoted_refund: reviewed.quote.total_refund,
                })
            }
        }
        A::Sell => {
            if !ui.driving.sell_confirm {
                ui.driving.sell_confirm = true;
                s.last_message="Review the sale value, then press Confirm vehicle sale. The vehicle must be empty and unassigned.".into();
                None
            } else {
                ui.driving.sell_confirm = false;
                wrap(TransportCommand::SellVehicle {
                    vehicle_id: v?.unit_id,
                    facility_id: g?,
                })
            }
        }
        A::Refuel => {
            let litres = fuel_offer(s, ui);
            if litres == 0 {
                return None;
            }
            wrap(TransportCommand::Refuel {
                vehicle_id: v?.unit_id,
                facility_id: g?,
                litres,
            })
        }
        A::Repair => wrap(TransportCommand::Repair {
            vehicle_id: v?.unit_id,
            facility_id: g?,
        }),
        A::Paint => {
            let displayed = ui.driving.displayed_paint.take();
            if fixed_vehicle_livery(s, ui, scenes) {
                s.last_message =
                    "The active model has a fixed livery. Paint is unavailable.".into();
                return None;
            }
            if displayed.is_none() || displayed != paint_context(s, ui, scenes) {
                s.last_message =
                    "The vehicle or appearance changed. Review Garage before applying paint."
                        .into();
                return None;
            }
            if unavailable(A::Paint, s, ui, scenes) {
                s.last_message =
                    "Paint requires 5 credits and you and the stopped vehicle at the garage."
                        .into();
                return None;
            }
            let v = v?;
            wrap(TransportCommand::Paint {
                vehicle_id: v.unit_id,
                facility_id: g?,
                paint: (v.paint + 1) % 8,
            })
        }
        A::UpgradeEngine | A::UpgradeBrakes => wrap(TransportCommand::Upgrade {
            vehicle_id: v?.unit_id,
            facility_id: g?,
            engine: matches!(action, A::UpgradeEngine),
        }),
        A::NextLicense => {
            ui.driving.license = (ui.driving.license + 1) % 3;
            None
        }
        A::License => wrap(TransportCommand::BuyLicense {
            license: license(ui),
            facility_id: g?,
        }),
        A::Lights | A::LeftSignal | A::RightSignal | A::Hazards | A::ParkingBrake => {
            let mut v = active_vehicle(s)?.clone();
            match action {
                A::Lights => v.lights = !v.lights,
                A::ParkingBrake => v.parking_brake = !v.parking_brake,
                _ => {
                    let requested = match action {
                        A::LeftSignal => TurnIndicator::Left,
                        A::RightSignal => TurnIndicator::Right,
                        _ => TurnIndicator::Hazard,
                    };
                    v.indicator = if v.indicator == requested {
                        TurnIndicator::Off
                    } else {
                        requested
                    };
                }
            }
            wrap(TransportCommand::SetSignals {
                vehicle_id: v.unit_id,
                lights: v.lights,
                indicator: v.indicator,
                parking_brake: v.parking_brake,
            })
        }
        A::BuyTrailer => wrap(TransportCommand::BuyTrailer { facility_id: g? }),
        A::Hitch => {
            let v = v?;
            let p = s.state.living.units.get(&v.unit_id)?.position;
            let id = s
                .state
                .transport
                .trailers
                .values()
                .filter(|t| {
                    t.owner == s.player_id
                        && t.attached_to.is_none()
                        && within(p, t.position, 20.)
                        && distance_to(s, t.position) <= 20.
                })
                .min_by(|a, b| {
                    a.position
                        .surface_distance_m(p)
                        .total_cmp(&b.position.surface_distance_m(p))
                })?
                .id;
            wrap(TransportCommand::HitchTrailer {
                vehicle_id: v.unit_id,
                trailer_id: id,
            })
        }
        A::Unhitch => wrap(TransportCommand::UnhitchTrailer {
            vehicle_id: v?.unit_id,
        }),
        A::NextCourse => {
            ui.driving.course = next(
                s.state.transport.courses.keys().copied().collect(),
                course(s, ui).map(|c| c.id),
            );
            ui.driving.route_to_garage = false;
            ui.driving.route_to_course = true;
            None
        }
        A::RouteCourse => {
            ui.driving.route_to_garage = false;
            ui.driving.route_to_course = true;
            s.last_message =
                "Course start marked. Enter an unassigned vehicle and drive within 20 m to begin."
                    .into();
            None
        }
        A::StartRace => {
            ui.driving.route_to_garage = false;
            ui.driving.route_to_course = false;
            wrap(TransportCommand::StartRace {
                course_id: course(s, ui)?.id,
                vehicle_id: v?.unit_id,
            })
        }
        A::AbandonRace => wrap(TransportCommand::AbandonRace),
    }
}

pub fn unavailable(
    action: DrivingAction,
    s: &SandboxRuntime,
    ui: &UiState,
    scenes: Option<&ActorSceneOverrides>,
) -> bool {
    use DrivingAction as A;
    if !s.state.transport.enabled {
        return true;
    }
    let v = vehicle(s, ui);
    let j = job(s, ui);
    let g = garage(s, ui);
    let account = s.state.living.accounts.get(&s.player_id);
    let money = account.map_or(0, |a| a.money);
    let driver = s.state.transport.drivers.get(&s.player_id);
    let far = g.is_none_or(|g| distance_to(s, g.position) > 20.);
    let service_far = far
        || v.is_none_or(|v| {
            s.state.living.units.get(&v.unit_id).is_none_or(|u| {
                g.is_none_or(|g| {
                    u.position
                        .surface_distance_m(g.position)
                        .hypot(u.position.altitude_m - g.position.altitude_m)
                        > 20.
                })
            }) || speed(s, v.unit_id) > 0.5
        });
    match action {
        A::NextVehicle => v.is_none(),
        A::NextJob => j.is_none(),
        A::NextGarage | A::RouteGarage => g.is_none(),
        A::Enter => v.is_none_or(|v| {
            s.state.is_company_vehicle(v.unit_id)
                || !licensed(s, v)
                || s.state
                    .living
                    .units
                    .get(&v.unit_id)
                    .is_none_or(|u| distance_to(s, u.position) > 20.)
                || speed(s, v.unit_id) > 0.5
        }),
        A::Exit => account.is_none_or(|a| {
            !s.state.transport.vehicles.contains_key(&a.active_unit)
                || speed(s, a.active_unit) > 0.5
        }),
        A::Accept => j.is_none_or(|j| j.assignment.is_some() || job_blocker(s, j, v).is_some()),
        A::Service | A::ServiceActive(_) => {
            j.is_none_or(|j| {
                j.assignment.as_ref().is_none_or(|a| {
                    a.owner != s.player_id
                        || account.is_none_or(|account| account.active_unit != a.vehicle_id)
                        || speed(s, a.vehicle_id) > 0.5
                })
            }) || job_target(s, ui).is_none_or(|(_, p)| distance_to(s, p) > 20.)
        }
        A::Cancel => j.is_none_or(|j| j.assignment.as_ref().is_none_or(|a| a.owner != s.player_id)),
        A::Autopilot => {
            v.is_none_or(|v| {
                !licensed(s, v)
                    || v.fuel_l <= 0.
                    || v.rental.as_ref().is_some_and(|r| r.remaining_seconds <= 0.)
                    || v.condition <= 0.01
                    || s.state.living.units.get(&v.unit_id).is_none_or(|u| {
                        target(s, ui).is_none_or(|(_, p)| u.position.surface_distance_m(p) > 2000.)
                    })
                    || s.state
                        .transport
                        .races
                        .values()
                        .any(|r| r.vehicle_id == v.unit_id)
            }) || target(s, ui).is_none()
        }
        A::Buy => {
            let spec = VEHICLE_MODELS[ui.driving.model % VEHICLE_MODELS.len()].spec();
            far || money < spec.price
                || g.is_none_or(|g| g.fuel_l < spec.fuel_capacity_l * 0.65)
                || driver.is_none_or(|d| !d.licenses.contains(&spec.required_license))
        }
        A::Rent => {
            let model = VEHICLE_MODELS[ui.driving.model % VEHICLE_MODELS.len()];
            let offer = rental_offer(model);
            far || money < offer.total
                || g.is_none_or(|g| g.fuel_l < 10.)
                || driver.is_none_or(|d| !d.licenses.contains(&model.spec().required_license))
        }
        A::RenewRental => v.is_none_or(|v| {
            v.rental
                .as_ref()
                .is_none_or(|r| r.remaining_seconds > 3300.)
                || money < rental_offer(v.model).fee
        }),
        A::ReturnRental => {
            service_far
                || v.is_none_or(|v| {
                    v.rental
                        .as_ref()
                        .is_none_or(|r| g.is_none_or(|g| r.facility_id != g.id))
                        || account.is_some_and(|a| a.active_unit == v.unit_id)
                        || assigned(s, v.unit_id)
                        || !v.cargo.is_empty()
                        || v.trailer.is_some()
                        || s.state
                            .transport
                            .races
                            .values()
                            .any(|r| r.vehicle_id == v.unit_id)
                })
        }
        A::Sell => {
            service_far
                || v.is_none_or(|v| {
                    v.rental.is_some()
                        || s.state.is_company_vehicle(v.unit_id)
                        || account.is_some_and(|a| a.active_unit == v.unit_id)
                        || !v.cargo.is_empty()
                        || v.trailer.is_some()
                        || s.state
                            .transport
                            .races
                            .values()
                            .any(|r| r.vehicle_id == v.unit_id)
                        || s.state.transport.jobs.values().any(|j| {
                            j.assignment
                                .as_ref()
                                .is_some_and(|a| a.vehicle_id == v.unit_id)
                        })
                })
        }
        A::Refuel => service_far || fuel_offer(s, ui) == 0,
        A::Repair => {
            service_far || v.is_none_or(|v| v.condition >= 0.999 || money < repair_price(v))
        }
        A::Paint => service_far || money < 500 || fixed_vehicle_livery(s, ui, scenes),
        A::UpgradeEngine | A::UpgradeBrakes => {
            service_far
                || v.is_none_or(|v| {
                    let level = if matches!(action, A::UpgradeEngine) {
                        v.engine_level
                    } else {
                        v.brakes_level
                    };
                    v.rental.is_some() || level >= 3 || money < upgrade_price(level)
                })
        }
        A::License => {
            let l = license(ui);
            far || money < l.cost()
                || driver
                    .is_none_or(|d| d.licenses.contains(&l) || d.jobs_completed < l.required_jobs())
        }
        A::BuyTrailer => far || money < 15000,
        A::Hitch => v.is_none_or(|v| {
            !v.model.can_tow()
                || v.trailer.is_some()
                || assigned(s, v.unit_id)
                || speed(s, v.unit_id) > 0.5
                || s.state.living.units.get(&v.unit_id).is_none_or(|u| {
                    !s.state.transport.trailers.values().any(|t| {
                        t.owner == s.player_id
                            && t.attached_to.is_none()
                            && within(u.position, t.position, 20.)
                            && distance_to(s, t.position) <= 20.
                    })
                })
        }),
        A::Unhitch => v.is_none_or(|v| {
            v.trailer.is_none()
                || !v.cargo.is_empty()
                || assigned(s, v.unit_id)
                || speed(s, v.unit_id) > 0.5
                || s.state
                    .living
                    .units
                    .get(&v.unit_id)
                    .is_none_or(|u| distance_to(s, u.position) > 20.)
        }),
        A::StartRace => {
            v.is_none_or(|v| {
                active_vehicle(s).is_none_or(|a| a.unit_id != v.unit_id)
                    || assigned(s, v.unit_id)
                    || v.fuel_l <= 0.
            }) || s.state.transport.races.contains_key(&s.player_id)
                || course(s, ui).is_none_or(|c| {
                    c.checkpoints
                        .first()
                        .is_none_or(|p| distance_to(s, *p) > 20.)
                })
        }
        A::AbandonRace => !s.state.transport.races.contains_key(&s.player_id),
        A::NextCourse | A::RouteCourse => course(s, ui).is_none(),
        A::Lights | A::LeftSignal | A::RightSignal | A::Hazards | A::ParkingBrake => {
            active_vehicle(s).is_none()
        }
        A::NextModel | A::PreviousModel | A::NextLicense => false,
    }
}

fn complete_body_transition(
    time: Res<Time>,
    mut ui: ResMut<UiState>,
    s: Res<SandboxRuntime>,
    mut controls: ResMut<PlayerControl>,
    mut queue: ResMut<ControlQueue>,
) {
    if let Some((id, mut elapsed)) = ui.driving.pending_body {
        elapsed += time.delta_secs();
        if s.state
            .living
            .accounts
            .get(&s.player_id)
            .is_some_and(|a| a.active_unit == id && a.possessed)
        {
            controls.selected.clear();
            controls.selected.insert(id);
            queue
                .0
                .push_back(ControlIntent::SetMode(ControlMode::ThirdPerson));
            ui.driving.pending_body = None;
        } else {
            ui.driving.pending_body = (elapsed < 4.).then_some((id, elapsed));
        }
    }
}
fn driving_keys(
    input: Res<crate::player_input::PlayerInput>,
    ui: Res<UiState>,
    s: Res<SandboxRuntime>,
    flow: Option<Res<crate::flow::GameFlow>>,
    mut queue: ResMut<interface::UiQueue>,
) {
    if input.world_blocked()
        || ui.editing_gps
        || flow.as_ref().is_some_and(|f| f.blocks_input())
        || s.state.living.accounts.get(&s.player_id).is_none_or(|a| {
            !a.possessed || !s.state.transport.vehicles.contains_key(&a.active_unit)
        })
    {
        return;
    }
    use crate::player_input::Binding;
    for (key, action) in [
        (Binding::Lights, DrivingAction::Lights),
        (Binding::Park, DrivingAction::ParkingBrake),
        (Binding::LeftSignal, DrivingAction::LeftSignal),
        (Binding::RightSignal, DrivingAction::RightSignal),
        (Binding::Hazards, DrivingAction::Hazards),
    ] {
        if input.just_pressed(key) {
            queue.0.push_back(Action::Driving(action));
        }
    }
}
/// Cached advisory routing through the loaded source graph with direct fallback.
/// Refresh after travel and at least every five seconds while parked, so changed
/// roads cannot leave stale guidance indefinitely. This does not enforce lane law.
#[derive(Resource, Default)]
struct NavigationPath {
    destination: Option<GeoPosition>,
    origin: Option<GeoPosition>,
    points: Vec<GeoPosition>,
    length_m: f64,
    road_guidance: bool,
    kind: Option<RoadPlanKind>,
    seconds: f32,
}
fn update_navigation_path(
    time: Res<Time>,
    ui: Res<UiState>,
    s: Res<SandboxRuntime>,
    flow: Option<Res<crate::flow::GameFlow>>,
    mut path: ResMut<NavigationPath>,
) {
    if flow.as_ref().is_some_and(|f| f.blocks_input()) {
        return;
    }
    let goal = target(&s, &ui).map(|(_, p)| p);
    let origin = s.state.living.active_unit(s.player_id).map(|u| u.position);
    path.seconds += time.delta_secs().min(0.25);
    if goal == path.destination && path.seconds < 1.0 {
        return;
    }
    if goal == path.destination
        && path.seconds < 5.0
        && origin
            .zip(path.origin)
            .is_some_and(|(a, b)| a.surface_distance_m(b) < 8.)
    {
        return;
    }
    path.seconds = 0.;
    path.destination = goal;
    path.origin = origin;
    path.points.clear();
    path.length_m = 0.;
    path.kind = None;
    path.road_guidance = false;
    let (Some(from), Some(to)) = (origin, goal) else {
        return;
    };
    let plan = s.state.living.vehicle_route_plan(from, to);
    install_navigation_plan(&mut path, from, plan);
}
fn install_navigation_plan(path: &mut NavigationPath, from: GeoPosition, plan: RoadPlan) {
    path.origin = Some(from);
    path.kind = Some(plan.kind);
    path.road_guidance = plan.kind == RoadPlanKind::Connected;
    path.points.clear();
    path.length_m = 0.;
    if plan.is_blocked() {
        return;
    }
    let points = plan.points;
    path.points.push(from);
    // Domain routes contain at most 128 graph points plus their exact goal.
    // Keep that final endpoint; dropping it understates the remaining distance.
    path.points.extend(points.into_iter().take(129));
    path.length_m = path
        .points
        .windows(2)
        .map(|p| p[0].surface_distance_m(p[1]))
        .sum();
}
fn guidance(path: &NavigationPath, heading: f32) -> String {
    match path.kind {
        Some(RoadPlanKind::BlockedAccess) => {
            return "Road access blocked · choose another route".into();
        }
        Some(RoadPlanKind::BlockedDirection) => {
            return "No permitted direction · choose another route".into();
        }
        Some(RoadPlanKind::Unavailable) => {
            return "Route data unavailable · automatic travel held".into();
        }
        _ => {}
    }
    let Some(from) = path.origin else {
        return String::new();
    };
    let Some(to) = path
        .points
        .iter()
        .copied()
        .find(|p| from.surface_distance_m(*p) > 12.)
    else {
        return "Stop at the marked destination".into();
    };
    let local = LocalFrame::new(from).to_local(to);
    let bearing = local[0].atan2(-local[2]).to_degrees();
    let turn = (bearing - f64::from(heading) + 540.).rem_euclid(360.) - 180.;
    let direction = if turn.abs() > 135. {
        "Turn back"
    } else if turn > 35. {
        "Bear right"
    } else if turn < -35. {
        "Bear left"
    } else {
        "Continue ahead"
    };
    format!(
        "{} · {} · {:.0} m remaining",
        match path.kind {
            Some(RoadPlanKind::DirectNoRoads) => "Direct bearing · no road data",
            Some(RoadPlanKind::DirectDisconnected) => "Direct bearing · roads disconnected",
            _ if path.road_guidance => "Route guidance",
            _ => "Direct bearing",
        },
        direction,
        path.length_m
    )
}

const MAP_SIZE: usize = 208;
const MAP_RADIUS_M: f32 = 350.;
#[derive(Component)]
struct RouteMap;
#[derive(Resource)]
struct RouteMapImage(Handle<Image>);
fn setup_route_map(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    use bevy::{
        asset::RenderAssetUsages,
        render::render_resource::{Extent3d, TextureDimension, TextureFormat},
    };
    let image = images.add(Image::new_fill(
        Extent3d {
            width: MAP_SIZE as u32,
            height: MAP_SIZE as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[18, 32, 29, 255],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    ));
    commands.insert_resource(RouteMapImage(image.clone()));
    commands
        .spawn((
            RouteMap,
            interface::AboveFooter(15.),
            Node {
                position_type: PositionType::Absolute,
                right: px(18),
                bottom: px(137),
                width: px(232),
                padding: UiRect::all(px(12)),
                flex_direction: FlexDirection::Column,
                row_gap: px(6),
                border_radius: BorderRadius::all(px(8)),
                display: Display::None,
                ..default()
            },
            BackgroundColor(interface::PANEL),
            GlobalZIndex(8),
        ))
        .with_children(|p| {
            p.spawn(interface::text(
                "NORTH UP · 700 m across",
                12.,
                interface::MUTED,
            ));
            p.spawn((
                ImageNode::new(image),
                Node {
                    width: px(MAP_SIZE as f32),
                    height: px(MAP_SIZE as f32),
                    ..default()
                },
            ));
            p.spawn(interface::text(
                "Gold · route   Blue · traffic\nSquares · service stops",
                11.,
                interface::MUTED,
            ));
        });
}
fn map_pixel(data: &mut [u8], x: i32, y: i32, color: [u8; 4], radius: i32) {
    if x < -radius
        || y < -radius
        || x >= MAP_SIZE as i32 + radius
        || y >= MAP_SIZE as i32 + radius
        || data.len() != MAP_SIZE * MAP_SIZE * 4
    {
        return;
    }
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            let (x, y) = (x + dx, y + dy);
            if x >= 0 && y >= 0 && x < MAP_SIZE as i32 && y < MAP_SIZE as i32 {
                let at = (y as usize * MAP_SIZE + x as usize) * 4;
                data[at..at + 4].copy_from_slice(&color);
            }
        }
    }
}
/// Clip before rasterizing so a distant geographic feature never creates an
/// unbounded line loop or an integer overflow in the map renderer.
fn map_line(data: &mut [u8], a: Vec2, b: Vec2, color: [u8; 4], radius: i32) {
    if !a.is_finite() || !b.is_finite() {
        return;
    }
    let d = b - a;
    if !d.is_finite() {
        return;
    }
    let mut lo = 0.0_f32;
    let mut hi = 1.0_f32;
    for (p, q) in [
        (-d.x, a.x),
        (d.x, MAP_SIZE as f32 - 1. - a.x),
        (-d.y, a.y),
        (d.y, MAP_SIZE as f32 - 1. - a.y),
    ] {
        if p.abs() < 1e-8 {
            if q < 0. {
                return;
            }
        } else {
            let r = q / p;
            if p < 0. {
                lo = lo.max(r);
            } else {
                hi = hi.min(r);
            }
            if lo > hi {
                return;
            }
        }
    }
    let start = a + d * lo;
    let end = a + d * hi;
    let n = (end - start)
        .abs()
        .max_element()
        .ceil()
        .clamp(1., MAP_SIZE as f32 * 2.) as usize;
    for i in 0..=n {
        let p = start.lerp(end, i as f32 / n as f32);
        map_pixel(data, p.x.round() as i32, p.y.round() as i32, color, radius);
    }
}
fn floating_huds_allowed(ui: &UiState) -> bool {
    // Drawers own their screen space while open. Keep the ordinary footer and
    // panel controls, and restore both driving cards when the drawers close.
    !ui.context_panel && !ui.world_panel && !ui.performance_panel
}

fn route_map(
    time: Res<Time>,
    ui: Res<UiState>,
    s: Res<SandboxRuntime>,
    control: Res<PlayerControl>,
    path: Res<NavigationPath>,
    flow: Option<Res<crate::flow::GameFlow>>,
    image: Res<RouteMapImage>,
    mut images: ResMut<Assets<Image>>,
    mut nodes: Query<&mut Node, With<RouteMap>>,
    mut seconds: Local<f32>,
) {
    let body = s.state.living.active_unit(s.player_id);
    let visible = control.mode.is_possessed()
        && floating_huds_allowed(&ui)
        && body.is_some_and(|u| s.state.transport.vehicles.contains_key(&u.id))
        && !flow.as_ref().is_some_and(|f| f.blocks_input());
    for mut node in &mut nodes {
        let display = if visible {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != display {
            node.display = display;
        }
    }
    *seconds += time.delta_secs().min(0.25);
    if !visible || *seconds < 0.25 {
        return;
    }
    *seconds = 0.;
    let Some(body) = body else {
        return;
    };
    let Some(mut image) = images.get_mut(&image.0) else {
        return;
    };
    let Some(data) = image.data.as_mut() else {
        return;
    };
    for pixel in data.chunks_exact_mut(4) {
        pixel.copy_from_slice(&[18, 32, 29, 255]);
    }
    let frame = LocalFrame::new(body.position);
    let project = |p| {
        let local = frame.to_local(p);
        Vec2::new(local[0] as f32, local[2] as f32) * (MAP_SIZE as f32 / (MAP_RADIUS_M * 2.))
            + Vec2::splat(MAP_SIZE as f32 * 0.5)
    };
    for road in s.state.living.roads.iter().take(64) {
        for pair in road.points.windows(2).take(63) {
            map_line(
                data,
                project(pair[0]),
                project(pair[1]),
                [113, 142, 128, 255],
                1,
            );
        }
    }
    for pair in path.points.windows(2).take(129) {
        map_line(
            data,
            project(pair[0]),
            project(pair[1]),
            [230, 195, 139, 255],
            1,
        );
    }
    for f in s.state.transport.facilities.values().take(64) {
        let p = project(f.position);
        if p.is_finite() {
            map_pixel(data, p.x as i32, p.y as i32, [230, 217, 182, 255], 2);
        }
    }
    for unit in s
        .state
        .living
        .units
        .values()
        .filter(|u| u.kind == terra_core::ActorKind::Vehicle && u.id != body.id)
        .take(128)
    {
        let p = project(unit.position);
        if p.is_finite() {
            map_pixel(data, p.x as i32, p.y as i32, [102, 184, 202, 255], 1);
        }
    }
    if let Some(goal) = path.destination {
        let p = project(goal);
        if p.is_finite() {
            map_pixel(data, p.x as i32, p.y as i32, [255, 195, 90, 255], 3);
        }
    }
    let center = Vec2::splat(MAP_SIZE as f32 * 0.5);
    let heading = body.heading_deg.to_radians();
    let forward = Vec2::new(heading.sin(), -heading.cos());
    let right = Vec2::new(forward.y, -forward.x);
    let tip = center + forward * 7.;
    let a = center - forward * 4. + right * 4.;
    let b = center - forward * 4. - right * 4.;
    for (a, b) in [(tip, a), (a, b), (b, tip)] {
        map_line(data, a, b, [240, 245, 228, 255], 1);
    }
}

fn setup_hud(mut commands: Commands) {
    commands
        .spawn((
            DrivingHud,
            interface::AboveFooter(15.),
            Node {
                position_type: PositionType::Absolute,
                left: px(18),
                bottom: px(137),
                width: px(318),
                padding: UiRect::all(px(14)),
                flex_direction: FlexDirection::Column,
                row_gap: px(5),
                border_radius: BorderRadius::all(px(8)),
                display: Display::None,
                ..default()
            },
            BackgroundColor(interface::PANEL),
            GlobalZIndex(9),
        ))
        .with_children(|p| {
            p.spawn((SpeedText, interface::text("0 km/h", 32., interface::TEXT)));
            p.spawn((NavigationText, interface::text("", 12., interface::SAND)));
        });
}
fn hud(
    ui: Res<UiState>,
    s: Res<SandboxRuntime>,
    control: Res<PlayerControl>,
    path: Res<NavigationPath>,
    flow: Option<Res<crate::flow::GameFlow>>,
    mut root: Query<&mut Node, With<DrivingHud>>,
    mut speeds: Query<&mut Text, (With<SpeedText>, Without<NavigationText>)>,
    mut routes: Query<&mut Text, (With<NavigationText>, Without<SpeedText>)>,
) {
    let v = s
        .state
        .living
        .active_unit(s.player_id)
        .and_then(|u| s.state.transport.vehicles.get(&u.id));
    let visible = v.is_some()
        && control.mode.is_possessed()
        && floating_huds_allowed(&ui)
        && !flow.as_ref().is_some_and(|f| f.blocks_input());
    for mut node in &mut root {
        node.display = if visible {
            Display::Flex
        } else {
            Display::None
        };
    }
    let Some(v) = v else {
        return;
    };
    for mut text in &mut speeds {
        *text = Text::new(format!(
            "{:03.0} km/h  {}",
            speed(&s, v.unit_id) * 3.6,
            if v.dynamics.gear < 0 {
                "R".into()
            } else if v.dynamics.gear == 0 {
                "N".into()
            } else {
                format!("D{}", v.dynamics.gear)
            }
        ));
    }
    for mut text in &mut routes {
        *text=Text::new(format!("{} · {:.1} L · {:.0}% condition{}\n{}\n{}",v.model.spec().name,v.fuel_l,v.condition*100.,if v.parking_brake{" · PARK"}else{""},target(&s,&ui).map(|(name,p)|format!("{name} · {:.0} m\n{}",distance_to(&s,p),guidance(&path,s.state.living.units.get(&v.unit_id).map_or(0.,|u|u.heading_deg)))).unwrap_or_else(||"Choose a job in Driving [V]".into()),driver_body(&s).map(|u|format!("Driver · food {:.0}% / water {:.0}% / energy {:.0}%\nPark and exit to eat, drink or rest.",u.needs.food*100.,u.needs.water*100.,u.needs.energy*100.)).unwrap_or_default()));
    }
}
fn navigation(
    ui: Res<UiState>,
    s: Res<SandboxRuntime>,
    runtime: Res<WorldRuntime>,
    resident: Res<ResidentChunks>,
    route: Res<NavigationPath>,
    mut gizmos: Gizmos,
    flow: Option<Res<crate::flow::GameFlow>>,
) {
    if flow.as_ref().is_some_and(|f| f.blocks_input()) {
        return;
    }
    if ui.panel != Panel::Driving
        && ui.panel != Panel::Garage
        && !ui.driving.route_to_garage
        && !ui.driving.route_to_course
        && s.state.transport.races.get(&s.player_id).is_none()
        && job(&s, &ui).is_none_or(|j| j.assignment.is_none())
    {
        return;
    }
    let Some((_, mut destination)) = target(&s, &ui) else {
        return;
    };
    let weather = runtime.effective_weather();
    let Some(h) = physical_surface(&resident, destination, &weather).map(|s| s.terrain_height_m)
    else {
        return;
    };
    destination.altitude_m = h + 0.3;
    let p = Vec3::from_array(runtime.frame.to_local(destination).map(|x| x as f32));
    let color = Color::srgb(0.98, 0.76, 0.37);
    gizmos.circle(
        Isometry3d::new(p, Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
        8.,
        color,
    );
    gizmos.line(p, p + Vec3::Y * 16., color);
    let mut budget = 256usize;
    for pair in route.points.windows(2) {
        if budget == 0 {
            break;
        }
        let start = pair[0];
        let end = pair[1];
        let frame = LocalFrame::new(start);
        let delta = frame.to_local(end);
        let count = (start.surface_distance_m(end) / 6.).ceil().clamp(1., 128.) as usize;
        for n in (0..count).step_by(2) {
            if budget == 0 {
                break;
            }
            budget -= 1;
            let point = |i: usize| {
                frame
                    .to_geo(delta.map(|x| x * i as f64 / count as f64))
                    .ok()
                    .and_then(|mut p| {
                        p.altitude_m =
                            physical_surface(&resident, p, &weather)?.terrain_height_m + 0.38;
                        Some(Vec3::from_array(
                            runtime.frame.to_local(p).map(|x| x as f32),
                        ))
                    })
            };
            if let (Some(a), Some(b)) = (point(n), point((n + 1).min(count))) {
                gizmos.line(a, b, color.with_alpha(0.72));
            }
        }
    }
}

fn race_gates(
    time: Res<Time>,
    s: Res<SandboxRuntime>,
    mut queue: ResMut<SandboxQueue>,
    mut cooldown: Local<f32>,
    flow: Option<Res<crate::flow::GameFlow>>,
) {
    *cooldown = (*cooldown - time.delta_secs()).max(0.);
    if *cooldown > 0. || flow.as_ref().is_some_and(|f| f.blocks_input()) {
        return;
    }
    if let Some(run) = s.state.transport.races.get(&s.player_id) {
        if let Some(c) = s.state.transport.courses.get(&run.course_id) {
            if c.checkpoints
                .get(run.next_checkpoint)
                .is_some_and(|p| distance_to(&s, *p) <= c.radius_m)
            {
                queue
                    .0
                    .push_back(SandboxIntent::Command(Command::Transport {
                        action: TransportCommand::RaceCheckpoint,
                    }));
                *cooldown = 1.;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn route_map_clips_distant_geometry_and_navigation_uses_geographic_bearings() {
        let mut pixels = vec![0; MAP_SIZE * MAP_SIZE * 4];
        map_line(
            &mut pixels,
            Vec2::new(-1e6, 104.),
            Vec2::new(1e6, 104.),
            [255; 4],
            0,
        );
        assert_eq!(
            pixels.chunks_exact(4).filter(|p| p[0] == 255).count(),
            MAP_SIZE
        );
        let before = pixels.clone();
        map_line(&mut pixels, Vec2::splat(f32::NAN), Vec2::ZERO, [1; 4], 1);
        map_pixel(&mut pixels, i32::MAX, i32::MIN, [1; 4], 3);
        assert_eq!(pixels, before);
        let from = GeoPosition::new(48., 11., 0.).unwrap();
        let east = LocalFrame::new(from).to_geo([100., 0., 0.]).unwrap();
        let path = NavigationPath {
            origin: Some(from),
            destination: Some(east),
            points: vec![from, east],
            length_m: 100.,
            road_guidance: true,
            ..default()
        };
        assert!(guidance(&path, 0.).contains("right"));
        assert!(guidance(&path, 180.).contains("left"));
        assert!(guidance(&path, 90.).contains("ahead"));
        assert_eq!(
            guidance(&path, 270.),
            "Route guidance · Turn back · 100 m remaining"
        );
    }
    #[test]
    fn blocked_source_routes_have_a_reason_and_never_draw_a_direct_navigation_line() {
        let from = GeoPosition::new(48., 11., 0.).unwrap();
        let to = LocalFrame::new(from).to_geo([100., 0., 0.]).unwrap();
        let mut world = LivingWorld {
            roads: vec![RoadRoute {
                id: "test:one-way".into(),
                points: vec![from, to],
                width_m: 6.,
                rules: RoadRules {
                    direction: RoadDirection::Forward,
                    ..Default::default()
                },
            }],
            ..Default::default()
        };
        let mut path = NavigationPath {
            destination: Some(from),
            ..Default::default()
        };
        install_navigation_plan(&mut path, to, world.vehicle_route_plan(to, from));
        assert!(path.points.is_empty());
        assert_eq!(path.length_m, 0.);
        assert!(guidance(&path, 270.).starts_with("No permitted direction"));
        world.roads[0].rules.motor_access = false;
        install_navigation_plan(&mut path, from, world.vehicle_route_plan(from, to));
        assert!(path.points.is_empty());
        assert!(guidance(&path, 90.).starts_with("Road access blocked"));
        world.roads[0].rules.speed_limit_m_s = Some(f32::NAN);
        install_navigation_plan(&mut path, from, world.vehicle_route_plan(from, to));
        assert!(path.points.is_empty());
        assert!(guidance(&path, 90.).starts_with("Route data unavailable"));
    }
    fn fixture() -> SandboxRuntime {
        let origin = GeoPosition::new(48.137, 11.575, 100.).unwrap();
        let mut s = SandboxRuntime::new(origin);
        s.state.seed_living_demo(origin, &[], &s.catalog).unwrap();
        s.state.ensure_transport_demo(&s.catalog).unwrap();
        s
    }

    #[test]
    fn floating_driving_huds_yield_to_drawers_and_return_when_closed() {
        let mut s = fixture();
        let vehicle = vehicle(&s, &UiState::default()).unwrap().unit_id;
        let account = s.state.living.accounts.get_mut(&s.player_id).unwrap();
        account.active_unit = vehicle;
        account.possessed = true;
        let mut app = App::new();
        app.insert_resource(s)
            .init_resource::<UiState>()
            .init_resource::<PlayerControl>()
            .init_resource::<NavigationPath>()
            .init_resource::<Time>()
            .init_resource::<Assets<Image>>()
            .add_systems(Startup, (setup_hud, setup_route_map))
            .add_systems(Update, (hud, route_map));
        for mode in [ControlMode::ThirdPerson, ControlMode::FirstPerson] {
            app.world_mut().resource_mut::<PlayerControl>().mode = mode;
            // Every individual drawer and every combination must take priority;
            // closing all of them restores the same cards without respawning.
            for mask in [0_u8, 1, 0, 2, 0, 4, 3, 5, 6, 7, 0] {
                let mut ui = app.world_mut().resource_mut::<UiState>();
                ui.world_panel = mask & 1 != 0;
                ui.context_panel = mask & 2 != 0;
                ui.performance_panel = mask & 4 != 0;
                app.update();
                let expected = if mask == 0 {
                    Display::Flex
                } else {
                    Display::None
                };
                let world = app.world_mut();
                let speed_card = world
                    .query_filtered::<&Node, With<DrivingHud>>()
                    .single(world)
                    .unwrap();
                assert_eq!(
                    speed_card.display, expected,
                    "driving card, drawer mask {mask}"
                );
                let route_card = world
                    .query_filtered::<&Node, With<RouteMap>>()
                    .single(world)
                    .unwrap();
                assert_eq!(
                    route_card.display, expected,
                    "route card, drawer mask {mask}"
                );
                let ui = world.resource::<UiState>();
                assert_eq!(
                    (ui.world_panel, ui.context_panel, ui.performance_panel),
                    (mask & 1 != 0, mask & 2 != 0, mask & 4 != 0),
                    "HUD visibility must not close the player's panel controls"
                );
            }
        }
        app.world_mut().resource_mut::<PlayerControl>().mode = ControlMode::Rts;
        app.update();
        let world = app.world_mut();
        for node in world
            .query_filtered::<&Node, Or<(With<DrivingHud>, With<RouteMap>)>>()
            .iter(world)
        {
            assert_eq!(
                node.display,
                Display::None,
                "RTS keeps its existing HUD behavior"
            );
        }
    }

    fn paint_fixture() -> (SandboxRuntime, UiState, u64) {
        let mut s = fixture();
        let mut ui = UiState {
            panel: Panel::Garage,
            context_panel: true,
            ..Default::default()
        };
        let garage = garage(&s, &ui).unwrap().clone();
        s.state
            .set_player_position(s.player_id, garage.position)
            .unwrap();
        let before = s.state.living.accounts[&s.player_id].money;
        let rented = s
            .state
            .apply(
                s.player_id,
                Command::Transport {
                    action: TransportCommand::RentVehicle {
                        model: VehicleModel::Compact,
                        facility_id: garage.id,
                    },
                },
                &s.catalog,
            )
            .unwrap()
            .created_id
            .unwrap();
        assert_eq!(
            s.state.living.accounts[&s.player_id].money,
            before - rental_offer(VehicleModel::Compact).total
        );
        ui.driving.vehicle = Some(rented);
        (s, ui, rented)
    }

    fn accepted_stop_fixture() -> (SandboxRuntime, UiState, u64, u64, u64) {
        let mut s = fixture();
        let vehicle = s
            .state
            .transport
            .vehicles
            .values()
            .find(|v| v.owner == s.player_id && TransportJobKind::Hitchhiker.accepts(v.model))
            .unwrap()
            .unit_id;
        let job = s
            .state
            .transport
            .jobs
            .values()
            .find(|j| j.kind == TransportJobKind::Hitchhiker)
            .unwrap()
            .clone();
        let other = *s
            .state
            .transport
            .jobs
            .keys()
            .find(|id| **id != job.id)
            .unwrap();
        s.state
            .apply(
                s.player_id,
                Command::Transport {
                    action: TransportCommand::AcceptJob {
                        job_id: job.id,
                        vehicle_id: vehicle,
                    },
                },
                &s.catalog,
            )
            .unwrap();
        // Explicit fixture positions isolate input/transaction handoff; this is
        // not evidence of physical travel or an uncoached player journey.
        let position = s.state.living.units[&vehicle].position;
        s.state.set_player_position(s.player_id, position).unwrap();
        s.state
            .apply(
                s.player_id,
                Command::Transport {
                    action: TransportCommand::EnterVehicle {
                        vehicle_id: vehicle,
                    },
                },
                &s.catalog,
            )
            .unwrap();
        let pickup = s.state.transport.facilities[&job.pickup_facility].position;
        s.state.set_player_position(s.player_id, pickup).unwrap();
        let mut ui = UiState::default();
        ui.driving.job = Some(other);
        ui.driving.route_to_garage = true;
        (s, ui, vehicle, job.id, other)
    }

    #[test]
    fn interact_resumes_the_active_assignment_after_browsing_another_offer() {
        let (mut s, mut ui, vehicle, accepted, browsed) = accepted_stop_fixture();
        let other_before = s.state.transport.jobs[&browsed].clone();
        let mut actions = interface::UiQueue::default();
        assert!(handle_vehicle_interaction(&mut s, &mut ui, &mut actions));
        assert_eq!(ui.driving.vehicle, Some(vehicle));
        assert_eq!(ui.driving.job, Some(accepted));
        assert!(!ui.driving.route_to_garage && !ui.driving.route_to_course);
        let mut settings = crate::player_input::InputSettings::default();
        settings
            .bind(crate::player_input::Binding::Interact, KeyCode::KeyK)
            .unwrap();
        assert_eq!(
            label(DrivingLabel::Service, &s, &ui, None, &settings),
            "Collect cargo / board passengers [K]"
        );
        let Some(Action::Driving(action)) = actions.0.pop_front() else {
            panic!("the active accepted stop should be serviceable");
        };
        assert!(actions.0.is_empty());
        // A queued browse must not retarget the interaction key's stop.
        ui.driving.job = Some(browsed);
        let command = act(
            action,
            &mut ui,
            &mut s,
            &mut PlayerControl::default(),
            &mut ControlQueue::default(),
            None,
        )
        .unwrap();
        assert!(matches!(command, Command::Transport {
            action: TransportCommand::PickupJob { job_id }
        } if job_id == accepted));
        s.state.apply(s.player_id, command, &s.catalog).unwrap();
        assert_eq!(
            s.state.transport.jobs[&accepted]
                .assignment
                .as_ref()
                .unwrap()
                .phase,
            TransportJobPhase::ToDestination
        );
        assert_eq!(s.state.transport.jobs[&browsed], other_before);
        let loaded = s.state.clone();
        assert!(
            act(
                action,
                &mut ui,
                &mut s,
                &mut PlayerControl::default(),
                &mut ControlQueue::default(),
                None
            )
            .is_none()
        );
        assert_eq!(s.state, loaded, "an old pickup cannot become a delivery");
        ui.driving.job = Some(accepted);
        assert_eq!(
            label(DrivingLabel::Service, &s, &ui, None, &settings),
            "Deliver and receive payment [K]"
        );
    }

    #[test]
    fn interact_stop_rechecks_restore_and_explains_nonserviceable_states() {
        let (mut s, mut ui, vehicle, accepted, _) = accepted_stop_fixture();
        let mut actions = interface::UiQueue::default();
        handle_vehicle_interaction(&mut s, &mut ui, &mut actions);
        let Some(Action::Driving(action)) = actions.0.pop_front() else {
            panic!()
        };
        s.restore_revision += 1;
        let before = s.state.clone();
        assert!(
            act(
                action,
                &mut ui,
                &mut s,
                &mut PlayerControl::default(),
                &mut ControlQueue::default(),
                None
            )
            .is_none()
        );
        assert_eq!(s.state, before);
        assert!(s.last_message.contains("stop changed"));
        s.state
            .living
            .units
            .get_mut(&vehicle)
            .unwrap()
            .velocity_enu_m_s = [2., 0., 0.];
        handle_vehicle_interaction(&mut s, &mut ui, &mut actions);
        assert!(actions.0.is_empty());
        assert!(s.last_message.starts_with("Stop the vehicle"));
        s.state
            .living
            .units
            .get_mut(&vehicle)
            .unwrap()
            .velocity_enu_m_s = [0.; 3];
        let away = LocalFrame::new(s.state.living.units[&vehicle].position)
            .to_geo([100., 0., 0.])
            .unwrap();
        s.state.set_player_position(s.player_id, away).unwrap();
        handle_vehicle_interaction(&mut s, &mut ui, &mut actions);
        assert!(actions.0.is_empty());
        assert!(s.last_message.contains("stop within 20 m"));
        s.state
            .apply(
                s.player_id,
                Command::Transport {
                    action: TransportCommand::CancelJob { job_id: accepted },
                },
                &s.catalog,
            )
            .unwrap();
        handle_vehicle_interaction(&mut s, &mut ui, &mut actions);
        assert!(actions.0.is_empty());
        assert!(s.last_message.contains("no accepted transport job"));
    }

    fn review_rental_return(s: &mut SandboxRuntime, ui: &mut UiState) {
        refresh_displayed(s, ui, None);
        assert!(
            act(
                DrivingAction::ReturnRental,
                ui,
                s,
                &mut PlayerControl::default(),
                &mut ControlQueue::default(),
                None
            )
            .is_none()
        );
        assert!(ui.driving.rental_return_confirm);
        refresh_displayed(s, ui, None);
    }

    #[test]
    fn rental_return_requires_a_rendered_confirmation_and_retains_the_domain_quote() {
        let (mut s, mut ui, id) = paint_fixture();
        let before = s.state.clone();
        refresh_displayed(&s, &mut ui, None);
        for _ in 0..2 {
            assert!(
                act(
                    DrivingAction::ReturnRental,
                    &mut ui,
                    &mut s,
                    &mut PlayerControl::default(),
                    &mut ControlQueue::default(),
                    None
                )
                .is_none()
            );
        }
        assert_eq!(s.state, before, "two queued clicks cannot bypass review");
        assert!(ui.driving.rental_return_confirm);
        let quote = s.state.rental_return_quote(s.player_id, id).unwrap();
        refresh_displayed(&s, &mut ui, None);
        s.state
            .transport
            .vehicles
            .get_mut(&id)
            .unwrap()
            .rental
            .as_mut()
            .unwrap()
            .remaining_seconds -= 1.;
        let command = act(
            DrivingAction::ReturnRental,
            &mut ui,
            &mut s,
            &mut PlayerControl::default(),
            &mut ControlQueue::default(),
            None,
        )
        .unwrap();
        assert!(matches!(command, Command::Transport {
            action: TransportCommand::ReturnRental { vehicle_id, quoted_refund, .. }
        } if vehicle_id == id && quoted_refund == quote.total_refund));
        assert!(
            act(
                DrivingAction::ReturnRental,
                &mut ui,
                &mut s,
                &mut PlayerControl::default(),
                &mut ControlQueue::default(),
                None
            )
            .is_none()
        );
        // A post-dispatch change still reaches the existing authority guard.
        s.state.transport.vehicles.get_mut(&id).unwrap().fuel_l -= 1.;
        let changed = s.state.clone();
        assert!(s.state.apply(s.player_id, command, &s.catalog).is_err());
        assert_eq!(s.state, changed);
        review_rental_return(&mut s, &mut ui);
        let refund = s
            .state
            .rental_return_quote(s.player_id, id)
            .unwrap()
            .total_refund;
        let money = s.state.living.accounts[&s.player_id].money;
        let command = act(
            DrivingAction::ReturnRental,
            &mut ui,
            &mut s,
            &mut PlayerControl::default(),
            &mut ControlQueue::default(),
            None,
        )
        .unwrap();
        s.state.apply(s.player_id, command, &s.catalog).unwrap();
        assert!(!s.state.transport.vehicles.contains_key(&id));
        assert_eq!(s.state.living.accounts[&s.player_id].money, money + refund);
    }

    #[test]
    fn rental_return_rejects_changed_refund_selection_principal_garage_and_restore() {
        for changed in 0..6 {
            let (mut s, mut ui, id) = paint_fixture();
            review_rental_return(&mut s, &mut ui);
            match changed {
                0 => s.state.transport.vehicles.get_mut(&id).unwrap().fuel_l -= 1.,
                1 => s.restore_revision += 1,
                2 => s.player_id += 1,
                3 => {
                    ui.driving.vehicle = s
                        .state
                        .transport
                        .vehicles
                        .keys()
                        .find(|other| **other != id)
                        .copied()
                }
                4 => {
                    let mut other = garage(&s, &ui).unwrap().clone();
                    other.id += 10_000;
                    ui.driving.garage = Some(other.id);
                    s.state.transport.facilities.insert(other.id, other);
                }
                _ => {
                    // Equal total refunds do not authorize a changed breakdown.
                    let v = s.state.transport.vehicles.get_mut(&id).unwrap();
                    v.fuel_l -= 1.;
                    v.rental.as_mut().unwrap().bond_escrow += 150;
                }
            }
            let before = s.state.clone();
            assert!(
                act(
                    DrivingAction::ReturnRental,
                    &mut ui,
                    &mut s,
                    &mut PlayerControl::default(),
                    &mut ControlQueue::default(),
                    None
                )
                .is_none(),
                "changed review context {changed}"
            );
            assert!(!ui.driving.rental_return_confirm);
            assert!(s.last_message.contains("rental or refund changed"));
            assert_eq!(s.state, before);
        }
    }

    #[test]
    fn rental_return_never_follows_an_implicit_replacement_vehicle() {
        let (mut s, mut ui, id) = paint_fixture();
        let garage = garage(&s, &ui).unwrap().clone();
        let others: Vec<_> = s
            .state
            .transport
            .vehicles
            .values()
            .filter(|v| v.owner == s.player_id && v.unit_id != id)
            .map(|v| v.unit_id)
            .collect();
        // Fixture placement lets ordinary sales remove the starter fleet. The
        // remaining implicit choices are two real, paid rentals.
        for other in others {
            s.state.living.units.get_mut(&other).unwrap().position = garage.position;
            s.state
                .apply(
                    s.player_id,
                    Command::Transport {
                        action: TransportCommand::SellVehicle {
                            vehicle_id: other,
                            facility_id: garage.id,
                        },
                    },
                    &s.catalog,
                )
                .unwrap();
        }
        let second = s
            .state
            .apply(
                s.player_id,
                Command::Transport {
                    action: TransportCommand::RentVehicle {
                        model: VehicleModel::Compact,
                        facility_id: garage.id,
                    },
                },
                &s.catalog,
            )
            .unwrap()
            .created_id
            .unwrap();
        ui.driving.vehicle = None;
        assert_eq!(vehicle(&s, &ui).unwrap().unit_id, id);
        review_rental_return(&mut s, &mut ui);
        let quote = s.state.rental_return_quote(s.player_id, id).unwrap();
        s.state
            .apply(
                s.player_id,
                Command::Transport {
                    action: TransportCommand::ReturnRental {
                        vehicle_id: id,
                        facility_id: garage.id,
                        quoted_refund: quote.total_refund,
                    },
                },
                &s.catalog,
            )
            .unwrap();
        assert_eq!(vehicle(&s, &ui).unwrap().unit_id, second);
        let before = s.state.clone();
        assert!(
            act(
                DrivingAction::ReturnRental,
                &mut ui,
                &mut s,
                &mut PlayerControl::default(),
                &mut ControlQueue::default(),
                None
            )
            .is_none()
        );
        assert_eq!(s.state, before);
        assert!(s.state.transport.vehicles.contains_key(&second));
    }

    #[test]
    fn exiting_keeps_displayed_vehicle_and_nearby_reentry_without_overriding_explicit_choice() {
        for explicit_other in [false, true] {
            let (mut s, mut ui, driven) = paint_fixture();
            let other = s
                .state
                .transport
                .vehicles
                .values()
                .find(|v| v.owner == s.player_id && v.unit_id != driven)
                .unwrap()
                .unit_id;
            assert!(other < driven, "the rental is not the first owned vehicle");
            s.state
                .apply(
                    s.player_id,
                    Command::Transport {
                        action: TransportCommand::EnterVehicle { vehicle_id: driven },
                    },
                    &s.catalog,
                )
                .unwrap();
            // Explicit snapshot setup puts the active rental away from the
            // first owned vehicle. This tests selection and the real dismount
            // command, not driving distance, fuel use or player travel.
            let distant = LocalFrame::new(s.state.living.units[&other].position)
                .to_geo([500., 0., 0.])
                .unwrap();
            s.state.set_player_position(s.player_id, distant).unwrap();
            ui.driving.vehicle = explicit_other.then_some(other);
            let displayed = if explicit_other { other } else { driven };
            assert_eq!(vehicle(&s, &ui).unwrap().unit_id, displayed);
            assert!(!unavailable(DrivingAction::Exit, &s, &ui, None));
            let money = s.state.living.accounts[&s.player_id].money;
            let other_before = s.state.transport.vehicles[&other].clone();
            let mut control = PlayerControl::default();
            let mut queue = ControlQueue::default();
            let exit = act(
                DrivingAction::Exit,
                &mut ui,
                &mut s,
                &mut control,
                &mut queue,
                None,
            )
            .unwrap();
            assert_eq!(ui.driving.vehicle, Some(displayed));
            assert_eq!(s.state.living.accounts[&s.player_id].active_unit, driven);
            s.state.apply(s.player_id, exit, &s.catalog).unwrap();
            let account = &s.state.living.accounts[&s.player_id];
            assert_eq!(account.active_unit, account.avatar_unit);
            assert_eq!(vehicle(&s, &ui).unwrap().unit_id, displayed);
            assert_eq!(s.state.transport.vehicles[&other], other_before);
            if explicit_other {
                assert!(
                    unavailable(DrivingAction::Enter, &s, &ui, None),
                    "a deliberate distant selection must not be silently changed"
                );
            } else {
                assert!(
                    !unavailable(DrivingAction::Enter, &s, &ui, None),
                    "the just-exited car remains selected and within reach"
                );
                let enter = act(
                    DrivingAction::Enter,
                    &mut ui,
                    &mut s,
                    &mut control,
                    &mut queue,
                    None,
                )
                .unwrap();
                assert!(matches!(
                    &enter,
                    Command::Transport {
                        action: TransportCommand::EnterVehicle { vehicle_id }
                    } if *vehicle_id == driven
                ));
                s.state.apply(s.player_id, enter, &s.catalog).unwrap();
                assert_eq!(s.state.living.accounts[&s.player_id].active_unit, driven);
            }
            assert_eq!(s.state.living.accounts[&s.player_id].money, money);
        }
    }

    #[test]
    fn garage_fixed_livery_blocks_paid_paint_and_procedural_paint_still_applies() {
        use terra_bevy::render::ActorSceneAppearance;
        use terra_core::ActorKind;
        let (mut s, mut ui, id) = paint_fixture();
        let mut scenes = ActorSceneOverrides::default();
        scenes
            .set_vehicle_model(
                VehicleModel::Compact,
                ActorSceneAppearance::new(Handle::default()),
            )
            .unwrap();
        // Browsing a different showroom model must not select its paint policy.
        ui.driving.model = VEHICLE_MODELS
            .iter()
            .position(|m| *m == VehicleModel::Pickup)
            .unwrap();
        refresh_displayed(&s, &mut ui, Some(&scenes));
        assert!(unavailable(DrivingAction::Paint, &s, &ui, Some(&scenes)));
        assert!(
            label(
                DrivingLabel::Paint,
                &s,
                &ui,
                Some(&scenes),
                &Default::default()
            )
            .contains("fixed livery")
        );
        let before = s.state.clone();
        let mut control = PlayerControl::default();
        let mut queue = ControlQueue::default();
        assert!(
            act(
                DrivingAction::Paint,
                &mut ui,
                &mut s,
                &mut control,
                &mut queue,
                Some(&scenes)
            )
            .is_none()
        );
        assert_eq!(
            s.state, before,
            "blocked paint must never reach the paid command"
        );

        // An override for humans must not disable paint on a procedural car.
        scenes.remove_vehicle_model(VehicleModel::Compact);
        scenes
            .set_kind(
                ActorKind::Human,
                ActorSceneAppearance::new(Handle::default()),
            )
            .unwrap();
        refresh_displayed(&s, &mut ui, Some(&scenes));
        assert!(!unavailable(DrivingAction::Paint, &s, &ui, Some(&scenes)));
        let money = s.state.living.accounts[&s.player_id].money;
        let paint = s.state.transport.vehicles[&id].paint;
        let command = act(
            DrivingAction::Paint,
            &mut ui,
            &mut s,
            &mut control,
            &mut queue,
            Some(&scenes),
        )
        .unwrap();
        s.state.apply(s.player_id, command, &s.catalog).unwrap();
        assert_eq!(s.state.living.accounts[&s.player_id].money, money - 500);
        assert_eq!(s.state.transport.vehicles[&id].paint, (paint + 1) % 8);
        let painted = s.state.clone();
        assert!(
            act(
                DrivingAction::Paint,
                &mut ui,
                &mut s,
                &mut control,
                &mut queue,
                Some(&scenes)
            )
            .is_none()
        );
        assert_eq!(
            s.state, painted,
            "a queued duplicate cannot reuse the displayed action"
        );

        scenes
            .set_kind(
                ActorKind::Vehicle,
                ActorSceneAppearance::new(Handle::default()),
            )
            .unwrap();
        assert!(
            unavailable(DrivingAction::Paint, &s, &ui, Some(&scenes)),
            "generic vehicle overrides also supply a fixed livery"
        );
    }

    #[test]
    fn queued_paint_rechecks_pack_selection_restore_and_displayed_vehicle() {
        use terra_bevy::render::ActorSceneAppearance;
        for changed in 0..5 {
            let (mut s, mut ui, id) = paint_fixture();
            let mut scenes = ActorSceneOverrides::default();
            refresh_displayed(&s, &mut ui, Some(&scenes));
            assert!(!unavailable(DrivingAction::Paint, &s, &ui, Some(&scenes)));
            match changed {
                0 => scenes
                    .set_vehicle_model(
                        VehicleModel::Compact,
                        ActorSceneAppearance::new(Handle::default()),
                    )
                    .unwrap(),
                1 => {
                    ui.driving.vehicle = s
                        .state
                        .transport
                        .vehicles
                        .values()
                        .find(|v| v.owner == s.player_id && v.unit_id != id)
                        .map(|v| v.unit_id)
                }
                2 => s.restore_revision += 1,
                3 => s.state.transport.vehicles.get_mut(&id).unwrap().paint = 4,
                _ => ui.panel = Panel::Driving,
            }
            let before = s.state.clone();
            assert!(
                act(
                    DrivingAction::Paint,
                    &mut ui,
                    &mut s,
                    &mut PlayerControl::default(),
                    &mut ControlQueue::default(),
                    Some(&scenes)
                )
                .is_none(),
                "changed context {changed} must reject a queued paid click"
            );
            assert_eq!(s.state, before);
        }
    }

    #[test]
    fn refuel_offer_respects_cash_tank_station_and_whole_litres() {
        assert_eq!(refuel_quantity(30., 80., 149), 0);
        assert_eq!(refuel_quantity(30., 80., 450), 3);
        assert_eq!(refuel_quantity(1.9, 80., 25000), 1);
        assert_eq!(refuel_quantity(30., 2.9, 25000), 2);
        assert_eq!(refuel_quantity(30., 80., 25000), 10);
        assert_eq!(refuel_quantity(-1., 80., 25000), 0);
        assert_eq!(refuel_quantity(f64::NAN, 80., 25000), 0);
    }
    #[test]
    fn vehicle_controls_follow_authority_even_with_stale_showroom_selection() {
        let mut s = fixture();
        let id = *s.state.transport.vehicles.keys().next().unwrap();
        let account = s.state.living.accounts.get_mut(&s.player_id).unwrap();
        account.active_unit = id;
        account.possessed = true;
        let mut extra = s.state.transport.vehicles[&id].clone();
        extra.unit_id = 9999;
        s.state.transport.vehicles.insert(9999, extra);
        let mut ui = UiState::default();
        ui.driving.vehicle = Some(9999);
        let mut control = PlayerControl::default();
        let mut queue = ControlQueue::default();
        let cmd = act(
            DrivingAction::ParkingBrake,
            &mut ui,
            &mut s,
            &mut control,
            &mut queue,
            None,
        )
        .unwrap();
        assert!(
            matches!(cmd,Command::Transport{action:TransportCommand::SetSignals{vehicle_id,..}} if vehicle_id==id)
        );
        s.state
            .living
            .accounts
            .get_mut(&s.player_id)
            .unwrap()
            .possessed = false;
        assert!(unavailable(DrivingAction::ParkingBrake, &s, &ui, None));
    }
    #[test]
    fn course_navigation_exists_before_a_race_and_garage_can_replace_it() {
        let mut s = fixture();
        let mut ui = UiState::default();
        let mut control = PlayerControl::default();
        let mut queue = ControlQueue::default();
        act(
            DrivingAction::RouteCourse,
            &mut ui,
            &mut s,
            &mut control,
            &mut queue,
            None,
        );
        assert!(s.state.transport.races.is_empty());
        assert_eq!(
            target(&s, &ui).unwrap().1,
            course(&s, &ui).unwrap().checkpoints[0]
        );
        act(
            DrivingAction::RouteGarage,
            &mut ui,
            &mut s,
            &mut control,
            &mut queue,
            None,
        );
        assert!(!ui.driving.route_to_course);
        assert_eq!(
            target(&s, &ui).unwrap().1,
            garage(&s, &ui).unwrap().position
        );
    }
    #[test]
    fn rental_ui_reviews_costs_renews_expiry_and_returns_with_authoritative_refund() {
        let mut s = fixture();
        let mut ui = UiState {
            panel: Panel::Garage,
            context_panel: true,
            ..Default::default()
        };
        let mut control = PlayerControl::default();
        let mut queue = ControlQueue::default();
        let garage = garage(&s, &ui).unwrap().clone();
        s.state
            .set_player_position(s.player_id, garage.position)
            .unwrap();
        let before = s.state.living.accounts[&s.player_id].money;
        assert!(!unavailable(DrivingAction::Rent, &s, &ui, None));
        assert!(
            label(
                DrivingLabel::RentalOffer,
                &s,
                &ui,
                None,
                &Default::default()
            )
            .contains("61.80")
        );
        let command = act(
            DrivingAction::Rent,
            &mut ui,
            &mut s,
            &mut control,
            &mut queue,
            None,
        )
        .unwrap();
        let result = s.state.apply(s.player_id, command, &s.catalog).unwrap();
        let id = result.created_id.unwrap();
        ui.driving.vehicle = Some(id);
        assert!(unavailable(DrivingAction::Sell, &s, &ui, None));
        assert!(unavailable(DrivingAction::UpgradeEngine, &s, &ui, None));
        assert!(
            label(DrivingLabel::Rental, &s, &ui, None, &Default::default())
                .contains("300 s prepaid")
        );
        s.state
            .transport
            .vehicles
            .get_mut(&id)
            .unwrap()
            .rental
            .as_mut()
            .unwrap()
            .remaining_seconds = 0.;
        assert!(
            label(DrivingLabel::Vehicle, &s, &ui, None, &Default::default())
                .contains("Rental expired")
        );
        let renew = act(
            DrivingAction::RenewRental,
            &mut ui,
            &mut s,
            &mut control,
            &mut queue,
            None,
        )
        .unwrap();
        s.state.apply(s.player_id, renew, &s.catalog).unwrap();
        assert!(!unavailable(DrivingAction::ReturnRental, &s, &ui, None));
        refresh_displayed(&s, &mut ui, None);
        assert!(
            act(
                DrivingAction::ReturnRental,
                &mut ui,
                &mut s,
                &mut control,
                &mut queue,
                None,
            )
            .is_none()
        );
        refresh_displayed(&s, &mut ui, None);
        assert!(
            label(
                DrivingLabel::ReturnRental,
                &s,
                &ui,
                None,
                &Default::default()
            )
            .contains("Confirm")
        );
        let returned = act(
            DrivingAction::ReturnRental,
            &mut ui,
            &mut s,
            &mut control,
            &mut queue,
            None,
        )
        .unwrap();
        s.state.apply(s.player_id, returned, &s.catalog).unwrap();
        assert!(!s.state.transport.vehicles.contains_key(&id));
        assert_eq!(
            s.state.living.accounts[&s.player_id].money,
            before - 2 * rental_offer(VehicleModel::Compact).fee
        );
    }
}
