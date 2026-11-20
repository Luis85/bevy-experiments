//! Native presentation for ecology, community projects and funded work.
//! Rules remain in terra-sandbox; the UI submits the same commands as a client.
use crate::{
    control::{ControlIntent, ControlQueue, PlayerControl},
    interface::{self, Action, InputMode, Label, Panel, UiState},
};
use bevy::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
use terra_bevy::rules::{
    ActivityCommitments, ActivityGuidance, Command, GuideAction, GuideCompanySection, GuideSurface,
    GuideTrack, LivingCommand, ResourceNode, Settlement, SettlementCommand, SupplyContract,
    SupplyContractAction,
};
use terra_bevy::{ResidentChunks, SandboxRuntime, WorldRuntime};
use terra_core::{GeoPosition, LocalFrame};

pub const RESOURCE_ITEMS: [(&str, &str); 7] = [
    ("berries", "Berries"),
    ("fiber", "Wild flax"),
    ("iron_ore", "Iron ore"),
    ("fish", "Fishing"),
    ("wood", "Timber"),
    ("stone", "Stone"),
    ("water", "Fresh water"),
];
pub const CONTRIBUTIONS: [&str; 3] = ["planks", "stone_blocks", "tool"];

#[derive(Default)]
pub struct FrontierUi {
    pub guide_track: GuideTrack,
    pub guide: Option<ActivityGuidance>,
    guide_context: Option<(u64, u64)>,
    guide_actor: Option<u64>,
    pub journey_choices_open: bool,
    pub(crate) scroll_revision: u64,
    commitments: Option<ActivityCommitments>,
    commitments_context: Option<(u64, u64)>,
    displayed_contract: Option<SupplyContract>,
    pub contract: Option<u64>,
    pub resource: Option<String>,
    pub resource_kind: usize,
    pub settlement: Option<u64>,
    pub contribution: usize,
    pub faction_claim: bool,
    pub animal: Option<u64>,
    displayed_actions: [Option<FrontierReview>; 5],
}

/// Small, presentation-only identities retained from a rendered label through
/// activation and dispatch. No snapshot reserves goods, capacity or authority.
#[derive(Clone)]
struct FrontierReview {
    token: u64,
    context: FrontierContext,
}

#[derive(Clone)]
struct FrontierContext {
    player: u64,
    restore: u64,
    active: Option<u64>,
    target: FrontierTarget,
}

#[derive(Clone)]
enum FrontierTarget {
    Consume(crate::actor_summary::ConsumeTarget),
    Resource {
        selection: Option<String>,
        kind: usize,
        id: String,
        item: String,
        position: GeoPosition,
    },
    Territory {
        selection: Option<u64>,
        territory: Settlement,
        contribution: usize,
        amount: u32,
    },
    Journey {
        selection: Option<u64>,
        guide: ActivityGuidance,
    },
    Contract {
        selection: Option<u64>,
        contract: SupplyContract,
    },
}

impl FrontierContext {
    fn same_intent(&self, other: &Self) -> bool {
        if (self.player, self.restore, self.active) != (other.player, other.restore, other.active) {
            return false;
        }
        match (&self.target, &other.target) {
            (FrontierTarget::Consume(a), FrontierTarget::Consume(b)) => a == b,
            (
                FrontierTarget::Resource {
                    selection: a,
                    kind: ak,
                    id: ai,
                    item: at,
                    position: ap,
                },
                FrontierTarget::Resource {
                    selection: b,
                    kind: bk,
                    id: bi,
                    item: bt,
                    position: bp,
                },
            ) => (a, ak, ai, at, ap) == (b, bk, bi, bt, bp),
            (
                FrontierTarget::Territory {
                    selection: a,
                    territory: at,
                    contribution: ac,
                    amount: aa,
                },
                FrontierTarget::Territory {
                    selection: b,
                    territory: bt,
                    contribution: bc,
                    amount: ba,
                },
            ) => a == b && at == bt && ac == bc && aa == ba,
            (
                FrontierTarget::Journey {
                    selection: a,
                    guide: ag,
                },
                FrontierTarget::Journey {
                    selection: b,
                    guide: bg,
                },
            ) => a == b && ag.same_intent(bg),
            (
                FrontierTarget::Contract {
                    selection: a,
                    contract: ac,
                },
                FrontierTarget::Contract {
                    selection: b,
                    contract: bc,
                },
            ) => a == b && ac == bc,
            _ => false,
        }
    }
}

fn active_identity(s: &SandboxRuntime) -> Option<u64> {
    s.state
        .living
        .active_unit(s.player_id)
        .filter(|body| body.owner == Some(s.player_id))
        .map(|body| body.id)
}

fn review_index(action: FrontierAction) -> Option<usize> {
    match action {
        FrontierAction::EatBerries | FrontierAction::EatFish | FrontierAction::DrinkWater => {
            Some(0)
        }
        FrontierAction::GoResource | FrontierAction::GatherResource => Some(1),
        FrontierAction::GoSettlement
        | FrontierAction::Contribute
        | FrontierAction::Fund
        | FrontierAction::Upgrade => Some(2),
        FrontierAction::ContinueJourney => Some(3),
        FrontierAction::AcceptContract
        | FrontierAction::CompleteContract
        | FrontierAction::CancelContract
        | FrontierAction::GoContract => Some(4),
        _ => None,
    }
}

fn action_context(
    s: &SandboxRuntime,
    ui: &UiState,
    index: usize,
    displayed: bool,
) -> Option<FrontierContext> {
    if !s.state.players.contains_key(&s.player_id) || s.state.is_player_dormant(s.player_id) {
        return None;
    }
    let active = active_identity(s);
    let target = match index {
        0 => FrontierTarget::Consume(crate::actor_summary::ConsumeTarget::active_person(s)?),
        1 => {
            let node = resource(s, ui)?;
            FrontierTarget::Resource {
                selection: ui.frontier.resource.clone(),
                kind: ui.frontier.resource_kind,
                id: node.id.clone(),
                item: node.item.clone(),
                position: node.position,
            }
        }
        2 => {
            let territory = territory(s, ui)?.clone();
            let item = CONTRIBUTIONS[ui.frontier.contribution % CONTRIBUTIONS.len()];
            let amount = required_inventory(s, item)
                .min(5)
                .min(contribution_remaining(&territory, item));
            FrontierTarget::Territory {
                selection: ui.frontier.settlement,
                territory,
                contribution: ui.frontier.contribution,
                amount,
            }
        }
        3 => {
            let guide = if displayed {
                if ui.frontier.guide_context != Some((s.player_id, s.restore_revision))
                    || ui.frontier.guide_actor != active
                {
                    return None;
                }
                ui.frontier.guide.clone()?
            } else {
                s.state.activity_guidance(
                    s.player_id,
                    &s.catalog,
                    ui.frontier.guide_track,
                    ui.frontier.contract,
                )
            };
            guide.action.as_ref()?;
            FrontierTarget::Journey {
                selection: ui.frontier.contract,
                guide,
            }
        }
        4 => {
            let contract = if displayed {
                if ui.frontier.commitments_context != Some((s.player_id, s.restore_revision)) {
                    return None;
                }
                ui.frontier.displayed_contract.clone()?
            } else {
                contract(s, ui)?.clone()
            };
            FrontierTarget::Contract {
                selection: ui.frontier.contract,
                contract,
            }
        }
        _ => return None,
    };
    Some(FrontierContext {
        player: s.player_id,
        restore: s.restore_revision,
        active,
        target,
    })
}

/// Refresh only with the labels. The token is process-unique, so an old queued
/// click cannot acquire meaning from a newly created UI state or restored view.
pub(crate) fn refresh_displayed_actions(s: &SandboxRuntime, ui: &mut UiState) {
    for index in 0..ui.frontier.displayed_actions.len() {
        let context = action_context(s, ui, index, true);
        if context
            .as_ref()
            .zip(ui.frontier.displayed_actions[index].as_ref())
            .is_some_and(|(current, previous)| current.same_intent(&previous.context))
        {
            continue;
        }
        ui.frontier.displayed_actions[index] = context
            .and_then(|context| next_review_token().map(|token| FrontierReview { token, context }));
    }
}

fn next_review_token() -> Option<u64> {
    static NEXT_TOKEN: AtomicU64 = AtomicU64::new(1);
    let mut candidate = NEXT_TOKEN.load(Ordering::Relaxed);
    loop {
        let next = candidate.checked_add(1)?;
        match NEXT_TOKEN.compare_exchange_weak(
            candidate,
            next,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return Some(candidate),
            Err(current) => candidate = current,
        }
    }
}

pub(crate) fn bind_action(ui: &UiState, action: FrontierAction) -> Option<Action> {
    match review_index(action) {
        Some(index) => {
            ui.frontier.displayed_actions[index]
                .as_ref()
                .map(|review| Action::FrontierReviewed {
                    action,
                    token: review.token,
                })
        }
        None => Some(Action::Frontier(action)),
    }
}

pub(crate) fn requires_review(action: FrontierAction) -> bool {
    review_index(action).is_some()
}

pub(crate) fn act_reviewed(
    action: FrontierAction,
    token: u64,
    ui: &mut UiState,
    s: &mut SandboxRuntime,
    resident: &ResidentChunks,
    runtime: &WorldRuntime,
    control: &mut PlayerControl,
    queue: &mut ControlQueue,
) -> Option<Command> {
    let current = review_index(action).and_then(|index| {
        let shown = ui.frontier.displayed_actions[index].as_ref()?;
        let current = action_context(s, ui, index, false)?;
        (shown.token == token && shown.context.same_intent(&current)).then_some(())
    });
    if current.is_none() {
        s.last_message = "The displayed actor, destination or action changed. Review the current named target before choosing the action again.".into();
        return None;
    }
    act(action, ui, s, resident, runtime, control, queue)
}
#[derive(Clone, Copy)]
pub enum FrontierAction {
    ChooseJourney,
    SelectJourney(GuideTrack),
    ContinueJourney,
    ReviewAcceptedSupply,
    NextContract,
    AcceptContract,
    CompleteContract,
    CancelContract,
    GoContract,
    NextResourceKind,
    NextResource,
    GoResource,
    GatherResource,
    EatBerries,
    EatFish,
    DrinkWater,
    CraftRod,
    CookFish,
    SmeltIron,
    CraftIronTool,
    WatchAnimal,
    Found,
    ClaimOwner,
    NextSettlement,
    GoSettlement,
    NextContribution,
    Contribute,
    Fund,
    Upgrade,
}
#[derive(Clone, Copy)]
pub enum FrontierLabel {
    Journey,
    JourneyTrack,
    JourneyAction,
    Commitments,
    ReviewAcceptedSupply,
    Progress,
    Contract,
    ContractReadiness,
    Resource,
    ResourceKind,
    Survival,
    Crafting,
    Wildlife,
    Territory,
    Contribution,
    ClaimOwner,
}

#[derive(Component)]
pub(crate) struct JourneyChoices;

fn line(parent: &mut ChildSpawnerCommands, label: FrontierLabel, prominent: bool) {
    parent.spawn((
        Label::Frontier(label),
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
fn button(parent: &mut ChildSpawnerCommands, title: &str, action: FrontierAction) {
    interface::button(parent, title, Action::Frontier(action));
}
pub fn panels(parent: &mut ChildSpawnerCommands) {
    interface::page(parent, Panel::Activities, |p| {
        line(p, FrontierLabel::Progress, true);
        interface::heading(p, "Your next step");
        interface::button_with_label(
            p,
            "Choose a goal",
            Action::Frontier(FrontierAction::ChooseJourney),
            Some(Label::Frontier(FrontierLabel::JourneyTrack)),
        );
        p.spawn((
            JourneyChoices,
            Node {
                display: Display::None,
                width: percent(100.),
                flex_direction: FlexDirection::Column,
                row_gap: px(4.),
                ..Default::default()
            },
        ))
        .with_children(|choices| {
            for (left, right) in [
                (("Suggested", GuideTrack::Suggested), ("Supply", GuideTrack::Supply)),
                (("Home", GuideTrack::Home), ("Community", GuideTrack::Community)),
                (("Road", GuideTrack::Road), ("Company", GuideTrack::Company)),
            ] {
                interface::pair(
                    choices,
                    (left.0, Action::Frontier(FrontierAction::SelectJourney(left.1))),
                    (right.0, Action::Frontier(FrontierAction::SelectJourney(right.1))),
                );
            }
            choices.spawn(interface::text(
                "Suggested follows urgent needs and accepted work. Choosing a goal keeps every existing commitment.",
                12.,
                interface::MUTED,
            ));
        });
        interface::button_with_label(
            p,
            "Waiting for your world",
            Action::Frontier(FrontierAction::ContinueJourney),
            Some(Label::Frontier(FrontierLabel::JourneyAction)),
        );
        line(p, FrontierLabel::Journey, false);
        interface::heading(p, "Your commitments");
        line(p, FrontierLabel::Commitments, false);
        interface::button_with_label(
            p,
            "No accepted supply work",
            Action::Frontier(FrontierAction::ReviewAcceptedSupply),
            Some(Label::Frontier(FrontierLabel::ReviewAcceptedSupply)),
        );
        interface::pair(
            p,
            ("Find resources", Action::Panel(Panel::Nature)),
            ("Manage territory", Action::Panel(Panel::Territory)),
        );
        interface::heading(p, "Make a living on the road");
        p.spawn(interface::text("Drive the owned pickup to earn fares or deliver real goods. Garage offers prepaid rentals; Fleet lets you hire drivers, stock a depot and recover interrupted work.", 12., interface::MUTED));
        interface::pair(
            p,
            ("Take a driving job", Action::Panel(Panel::Driving)),
            ("Build a fleet", Action::Panel(Panel::Fleet)),
        );
        interface::button(p, "Visit garage and rentals", Action::Panel(Panel::Garage));
        interface::heading(p, "Supply noticeboard");
        line(p, FrontierLabel::Contract, true);
        line(p, FrontierLabel::ContractReadiness, false);
        button(p, "Next supply request", FrontierAction::NextContract);
        button(p, "Accept this contract", FrontierAction::AcceptContract);
        button(
            p,
            "Order active actor to destination",
            FrontierAction::GoContract,
        );
        button(
            p,
            "Deliver goods and collect reward",
            FrontierAction::CompleteContract,
        );
        button(
            p,
            "Return accepted contract",
            FrontierAction::CancelContract,
        );
        p.spawn(interface::text("Rewards are funded by the market. Goods enter real production stocks. Up to three contracts can be accepted at once.", 12., interface::MUTED));
        interface::pair(
            p,
            ("Open market", Action::Panel(Panel::Market)),
            ("Build and farm", Action::Panel(Panel::Build)),
        );
    });
    interface::page(parent, Panel::Nature, |p| {
        interface::button_with_label(
            p,
            "",
            Action::Frontier(FrontierAction::NextResourceKind),
            Some(Label::Frontier(FrontierLabel::ResourceKind)),
        );
        line(p, FrontierLabel::Resource, true);
        interface::pair(
            p,
            ("Next patch", Action::Frontier(FrontierAction::NextResource)),
            ("Go to patch", Action::Frontier(FrontierAction::GoResource)),
        );
        button(p, "Gather up to 5 units", FrontierAction::GatherResource);
        line(p, FrontierLabel::Survival, false);
        button(p, "Eat gathered berries", FrontierAction::EatBerries);
        button(p, "Eat cooked fish", FrontierAction::EatFish);
        button(p, "Drink fresh water", FrontierAction::DrinkWater);
        interface::heading(p, "Field equipment and processing");
        line(p, FrontierLabel::Crafting, false);
        button(p, "Craft fishing rod", FrontierAction::CraftRod);
        button(p, "Cook fish at workbench", FrontierAction::CookFish);
        button(p, "Smelt ore at workbench", FrontierAction::SmeltIron);
        button(
            p,
            "Craft iron tool at workbench",
            FrontierAction::CraftIronTool,
        );
        p.spawn(interface::text("Ore needs a tool. Fishing needs a rod. Finite deposits remain depleted; renewable patches recover with world time. Missing ingredients are reported without consuming anything.", 12., interface::MUTED));
        interface::heading(p, "Wildlife habitats");
        line(p, FrontierLabel::Wildlife, false);
        button(p, "Observe next wild animal", FrontierAction::WatchAnimal);
        p.spawn(interface::text("Observe is inspection only; gathering and trade still use your active owned actor. Nearby people and moving vehicles disturb wildlife. Populations and yields are authored game ecology, not observed census or mineral data.", 12., interface::MUTED));
    });
    interface::page(parent, Panel::Territory, |p| {
        line(p, FrontierLabel::Territory, true);
        interface::pair(
            p,
            (
                "Next territory",
                Action::Frontier(FrontierAction::NextSettlement),
            ),
            (
                "Go to center",
                Action::Frontier(FrontierAction::GoSettlement),
            ),
        );
        interface::heading(p, "Found a neighborhood");
        interface::button_with_label(
            p,
            "",
            Action::Frontier(FrontierAction::ClaimOwner),
            Some(Label::Frontier(FrontierLabel::ClaimOwner)),
        );
        button(p, "Name and found here", FrontierAction::Found);
        p.spawn(interface::text("Founding uses 20 wood, 10 stone and 50 credits. The credits become the public treasury. The 100 m claim must respect other owners' land.", 12., interface::MUTED));
        interface::heading(p, "Public works");
        interface::button_with_label(
            p,
            "",
            Action::Frontier(FrontierAction::NextContribution),
            Some(Label::Frontier(FrontierLabel::Contribution)),
        );
        button(
            p,
            "Contribute up to 5 materials",
            FrontierAction::Contribute,
        );
        button(p, "Fund treasury with 25 credits", FrontierAction::Fund);
        button(
            p,
            "Upgrade when requirements are met",
            FrontierAction::Upgrade,
        );
        p.spawn(interface::text("Neighborhood → Village → City. Larger charters expand territory and plot capacity. Contribute, fund and upgrade within 20 m of the center. Public stock is separate from your carried inventory. Build houses, plant farms and complete supply work to qualify.", 12., interface::MUTED));
        interface::pair(
            p,
            ("Build housing", Action::Panel(Panel::Build)),
            ("Faction roles", Action::Panel(Panel::Factions)),
        );
    });
}

fn position(s: &SandboxRuntime) -> Option<GeoPosition> {
    s.state.players.get(&s.player_id).map(|p| p.position)
}
fn distance(a: GeoPosition, b: GeoPosition) -> f64 {
    a.surface_distance_m(b).hypot(a.altitude_m - b.altitude_m)
}
fn credits(value: u64) -> String {
    format!("{}.{:02}", value / 100, value % 100)
}

/// Guidance is a bounded presentation cache. Refresh during pause/menus too;
/// actual actions are always derived again before command submission.
pub(crate) fn refresh_guidance(
    sandbox: Res<SandboxRuntime>,
    mut ui: ResMut<UiState>,
    time: Res<Time<Real>>,
    mut elapsed: Local<f32>,
    mut choices: Query<&mut Node, With<JourneyChoices>>,
) {
    for mut node in &mut choices {
        node.display = if ui.frontier.journey_choices_open {
            Display::Flex
        } else {
            Display::None
        };
    }
    *elapsed += time.delta_secs();
    if *elapsed < 0.2
        && ui.frontier.guide.is_some()
        && ui.frontier.guide_context == Some((sandbox.player_id, sandbox.restore_revision))
        && ui.frontier.guide_actor == active_identity(&sandbox)
    {
        return;
    }
    *elapsed = 0.;
    cache_guidance(&sandbox, &mut ui);
}

fn cache_guidance(sandbox: &SandboxRuntime, ui: &mut UiState) {
    ui.frontier.guide = Some(sandbox.state.activity_guidance(
        sandbox.player_id,
        &sandbox.catalog,
        ui.frontier.guide_track,
        ui.frontier.contract,
    ));
    ui.frontier.guide_context = Some((sandbox.player_id, sandbox.restore_revision));
    ui.frontier.guide_actor = active_identity(sandbox);
    ui.frontier.commitments = sandbox.state.activity_commitments(sandbox.player_id);
    ui.frontier.commitments_context = Some((sandbox.player_id, sandbox.restore_revision));
    ui.frontier.displayed_contract = contract(sandbox, ui).cloned();
}

fn next_accepted_supply(ui: &UiState) -> Option<&SupplyContract> {
    let requests = &ui.frontier.commitments.as_ref()?.supply_requests;
    let next = requests
        .iter()
        .position(|request| Some(request.id) == ui.frontier.contract)
        .map_or(0, |index| (index + 1) % requests.len().max(1));
    requests.get(next)
}

fn reviewed_contract(s: &mut SandboxRuntime, ui: &mut UiState) -> Option<SupplyContract> {
    let current = contract(s, ui).cloned();
    if ui.frontier.commitments_context != Some((s.player_id, s.restore_revision))
        || ui.frontier.displayed_contract != current
    {
        cache_guidance(s, ui);
        ui.frontier.guide = None;
        ui.frontier.commitments_context = None;
        s.last_message = "The displayed supply request changed. Review its current goods, reward and destination before choosing an action.".into();
        return None;
    }
    current
}

fn ready_reviewed_contract(
    s: &mut SandboxRuntime,
    ui: &mut UiState,
    action: SupplyContractAction,
) -> Option<SupplyContract> {
    let contract = reviewed_contract(s, ui)?;
    match s
        .state
        .supply_contract_readiness(s.player_id, contract.id, action, &s.catalog)
    {
        Ok(()) => Some(contract),
        Err(reason) => {
            s.last_message = reason.to_string();
            None
        }
    }
}
fn items(items: &terra_bevy::rules::Inventory) -> String {
    items
        .iter()
        .map(|(k, v)| format!("{v} {}", k.replace('_', " ")))
        .collect::<Vec<_>>()
        .join(" · ")
}
pub fn contract<'a>(s: &'a SandboxRuntime, ui: &UiState) -> Option<&'a SupplyContract> {
    let all = &s.state.settlements.contracts;
    ui.frontier
        .contract
        .and_then(|id| all.get(&id))
        .filter(|c| c.claimed_by.is_none() || c.claimed_by == Some(s.player_id))
        .or_else(|| {
            all.values()
                .filter(|c| c.claimed_by == Some(s.player_id))
                .min_by(|a, b| a.expires_at.total_cmp(&b.expires_at).then(a.id.cmp(&b.id)))
        })
        .or_else(|| all.values().find(|c| c.claimed_by.is_none()))
}
pub fn territory<'a>(s: &'a SandboxRuntime, ui: &UiState) -> Option<&'a Settlement> {
    let all = &s.state.settlements.territories;
    match ui.frontier.settlement {
        Some(id) => all.get(&id),
        None => all
            .values()
            .find(|t| s.state.may_build(s.player_id, t.faction_owner, t.founder))
            .or_else(|| all.values().next()),
    }
}
pub fn resource<'a>(s: &'a SandboxRuntime, ui: &UiState) -> Option<&'a ResourceNode> {
    let item = RESOURCE_ITEMS[ui.frontier.resource_kind % RESOURCE_ITEMS.len()].0;
    let origin = position(s)?;
    match ui.frontier.resource.as_ref() {
        Some(id) => s.state.resource_nodes.get(id).filter(|r| r.item == item),
        None => s
            .state
            .resource_nodes
            .values()
            .filter(|r| r.item == item)
            .min_by(|a, b| distance(origin, a.position).total_cmp(&distance(origin, b.position))),
    }
}
fn required_inventory(s: &SandboxRuntime, item: &str) -> u32 {
    s.state
        .players
        .get(&s.player_id)
        .and_then(|p| p.inventory.get(item))
        .copied()
        .unwrap_or(0)
}
fn contribution_remaining(territory: &Settlement, item: &str) -> u32 {
    territory
        .tier
        .next()
        .and_then(|tier| tier.material_cost().get(item).copied())
        .unwrap_or(0)
        .saturating_sub(territory.stock.get(item).copied().unwrap_or(0))
}
fn contribute(s: &SandboxRuntime, ui: &UiState) -> Option<Command> {
    let territory = territory(s, ui)?;
    let item = CONTRIBUTIONS[ui.frontier.contribution % CONTRIBUTIONS.len()];
    Some(Command::Settlement {
        action: SettlementCommand::Contribute {
            settlement_id: territory.id,
            item: item.into(),
            amount: required_inventory(s, item)
                .min(5)
                .min(contribution_remaining(territory, item)),
        },
    })
}

fn supply_readiness_label(s: &SandboxRuntime, ui: &UiState, partial: bool) -> String {
    let Some(request) = ui.frontier.displayed_contract.as_ref() else {
        return String::new();
    };
    let accepted = request.claimed_by == Some(s.player_id);
    let action = if accepted {
        SupplyContractAction::Deliver
    } else {
        SupplyContractAction::Accept
    };
    match s.state.supply_contract_readiness(s.player_id, request.id, action, &s.catalog) {
        Ok(()) if accepted => {
            if partial { "Ready to deliver in this view. Current availability is checked again when delivery completes.".into() }
            else { "Ready to deliver. Exchange the requested goods for the reserved reward below.".into() }
        }
        Ok(()) => "Accept to take this request and refresh its delivery deadline. No carried goods are spent.".into(),
        Err(reason) => format!("{}: {reason}", if accepted { "Delivery blocked" } else { "Cannot accept" }),
    }
}

fn resource_blocker(s: &SandboxRuntime, ui: &UiState) -> Option<String> {
    let Some(resource) = resource(s, ui) else {
        return Some("Choose an available patch before gathering.".into());
    };
    if s.state.is_player_dormant(s.player_id) {
        return Some("Resume your session before gathering.".into());
    }
    if resource.remaining == 0 {
        return Some(
            if resource.regrow_seconds > 0. {
                "Patch depleted · wait for regrowth or choose Next patch."
            } else {
                "Deposit depleted · choose Next patch."
            }
            .into(),
        );
    }
    if s.state
        .check_settlement_interact(s.player_id, resource.position)
        .is_err()
    {
        return Some("Protected territory · owner or faction access required.".into());
    }
    if s.state.plots.values().any(|plot| {
        !s.state
            .may_interact(s.player_id, plot.faction_owner, plot.owner)
            && plot.center.surface_distance_m(resource.position) <= plot.radius_m
    }) {
        return Some("Protected plot · builder access required.".into());
    }
    if let Err(reason) = s.state.validate_ecology_gather(s.player_id, &resource.id) {
        return Some(format!("Cannot gather: {reason}"));
    }
    if position(s).is_none_or(|here| {
        distance(here, resource.position) > terra_bevy::rules::INTERACTION_REACH_M
    }) {
        return Some("Too far to gather · use Go to patch, then gather within 20 m.".into());
    }
    None
}

pub fn label(
    which: FrontierLabel,
    s: &SandboxRuntime,
    ui: &UiState,
    network: bool,
    partial: bool,
) -> String {
    let here = position(s);
    let partial = partial || s.state.is_partial();
    match which {
        FrontierLabel::JourneyTrack => format!("Goal: {} · {}", ui.frontier.guide_track.label(), if ui.frontier.journey_choices_open { "close choices" } else { "choose" }),
        FrontierLabel::JourneyAction => ui.frontier.guide.as_ref().map(|g| {
            if matches!(g.action, Some(GuideAction::Command(Command::Living { action: LivingCommand::Consume { .. } }))) {
                ui.frontier.guide_actor.and_then(|id|s.state.living.units.get(&id)).map(|body|format!("{} for {}",g.action_label,body.name)).unwrap_or_else(||"Review your active actor".into())
            } else { g.action_label.clone() }
        }).unwrap_or_else(||"Waiting for your world".into()),
        FrontierLabel::Commitments => ui.frontier.commitments.as_ref().map(|brief|brief.detail.clone()).unwrap_or_else(||"Your current work appears when your character is ready.".into()),
        FrontierLabel::ReviewAcceptedSupply => next_accepted_supply(ui).map(|request|format!("Review supply request #{}",request.id)).unwrap_or_else(||"No accepted supply work".into()),
        FrontierLabel::Progress => {
            let p=s.state.settlements.progress.get(&s.player_id);
            let completed=p.map_or(0,|p|p.contracts_completed);
            format!("{} · {} community XP\n{} supply contract{} completed", p.map_or("Newcomer",|p|p.rank()),p.map_or(0,|p|p.experience),completed,if completed==1 {""} else {"s"})
        }
        FrontierLabel::Survival => s.state.living.active_unit(s.player_id).filter(|u|u.owner==Some(s.player_id)).map(|u| {
            if crate::actor_summary::ConsumeTarget::active_person(s).is_some() {
                format!("Supplies for {} · active body\nFood {:.0}% · Water {:.0}%\nCarried: {} berries · {} cooked fish · {} water\nEat or drink when below 98%.",u.name,u.needs.food*100.,u.needs.water*100.,required_inventory(s,"berries"),required_inventory(s,"cooked_fish"),required_inventory(s,"water"))
            } else if s.state.is_player_dormant(s.player_id) { "Resume your session before using supplies.".into() }
            else {format!("{} is active. Park and exit on foot to use your driver's supplies.",u.name)}
        }).unwrap_or_else(||"Your active body is unavailable. Choose an owned person in Command before using supplies.".into()),
        FrontierLabel::Journey => {
            ui.frontier.guide.as_ref().map(|guide| {
                let description=guide.description();
                ui.frontier.guide_actor.and_then(|id|s.state.living.units.get(&id)).map(|body|format!("Active actor: {}\n{}",body.name,description)).unwrap_or(description)
            }).unwrap_or_else(||"Your next step appears when the world and your character are ready.".into())
        }
        FrontierLabel::Contract => ui.frontier.displayed_contract.as_ref().map(|c| {
            let target=s.state.living.producers.get(&c.target_site).map(|p|p.name.as_str()).or_else(||s.state.living.shops.get(&c.target_site).map(|p|p.name.as_str())).unwrap_or("Regional service");
            format!("{} · request #{}\nDeliver {} {} to {}\nYou carry {} · {:.0} m away\nGross reward {} cr + {} XP\nReward reserved · {}",if c.claimed_by==Some(s.player_id){"Accepted by you"}else{"Available"},c.id,c.amount,c.item,target,required_inventory(s,&c.item),here.map_or(0.,|p|distance(p,c.target_position)),credits(c.reward),c.experience,terra_bevy::rules::supply_deadline_label(c.expires_at-s.state.settlements.elapsed_seconds))
        }).unwrap_or_else(||"No funded requests are open. Producers post work when inputs run low and the market can fund the reward. The economy continues while you explore.".into()),
        FrontierLabel::ContractReadiness => supply_readiness_label(s, ui, network || partial),
        FrontierLabel::ResourceKind => format!("Find: {} · change",RESOURCE_ITEMS[ui.frontier.resource_kind % RESOURCE_ITEMS.len()].1),
        FrontierLabel::Resource => resource(s,ui).map(|r| {
            let requirement=s.state.ecology.resource_requirement(&r.id).map_or("Hands / ordinary gathering".into(),|tool|if tool=="tool"{format!("Needs tool or iron tool · own {} / {}",required_inventory(s,"tool"),required_inventory(s,"iron_tool"))}else{format!("Requires {} · you own {}",tool.replace('_'," "),required_inventory(s,tool))});
            let recovery=if r.regrow_seconds>0. {format!("Renewable · +1 every {:.0} world seconds",r.regrow_seconds)}else{"Finite deposit · no automatic refill".into()};
            let access=resource_blocker(s,ui).unwrap_or_else(||"Gather within 20 m · up to 5 units".into());
            format!("{} · {} / {} units · carry {}\n{:.0} m from active actor\n{}\n{}\n{}",r.item.replace('_'," "),r.remaining,r.capacity,required_inventory(s,&r.item),here.map_or(0.,|p|distance(p,r.position)),recovery,requirement,access)
        }).unwrap_or_else(||if ui.frontier.resource.is_some() {"The chosen patch is no longer in this view. Choose Next patch to review another, or explore back to this patch before gathering.".into()}else{"No matching patch is in the loaded region. Try another resource or explore a different habitat. Empty coverage is not invented as real source data.".into()}),
        FrontierLabel::Crafting => ["fishing_rod", "cooked_fish", "iron_ingot", "iron_tool"].into_iter().filter_map(|id| s.catalog.recipes.get(id).map(|r|format!("{}: {}{}",id.replace('_', " "),items(&r.inputs),if r.requires_workbench {" · workbench"}else{""}))).collect::<Vec<_>>().join("\n"),
        FrontierLabel::Wildlife => {
            let mut text=format!("{} wild animals{} · {} habitats\n",s.state.ecology.animals.len(),if network {" received"}else if partial{" active"}else{""},s.state.ecology.habitats.len());
            for species in terra_bevy::rules::AnimalSpecies::ALL {
                let count=s.state.ecology.animals.values().filter(|a|a.species==species).count();
                if count>0 {text.push_str(&format!("{}: {}  ",species.label(),count));}
            }
            if let Some(a)=ui.frontier.animal.and_then(|id|s.state.ecology.animals.get(&id)) {
                text.push_str(&format!("\nObserving {} · {}",a.species.label(),a.behavior.label()));
            }
            text.push_str(&format!("\n{} disturbance responses · {} recoveries",s.state.ecology.metrics.flee_events,s.state.ecology.metrics.population_recoveries)); text
        }
        FrontierLabel::ClaimOwner => if ui.frontier.faction_claim { format!("Owner: selected faction {} · change",ui.selected_faction.map(|x|format!("#{x}")).unwrap_or_else(||"(choose in Factions)".into())) } else {"Owner: you personally · change".into()},
        FrontierLabel::Territory => territory(s,ui).map(|t| {
            let owner=t.faction_owner.map(|id|format!("Faction #{id}")).unwrap_or_else(||format!("Player #{}",t.founder));
            let mut text=format!("{} · {}\n{} · {:.0} m radius\nTreasury {} cr · {:.0} m to center\nStored: {}",t.name,t.tier.label(),owner,t.tier.radius_m(),credits(t.treasury),here.map_or(0.,|p|distance(p,t.center)),if t.stock.is_empty(){"none".into()}else{items(&t.stock)});
            if let Ok(r)=s.state.settlement_readiness(s.player_id,t.id) {
                text.push_str(&format!("\nPlots {} / {}",r.plots,t.tier.plot_capacity()));
                if partial {text.push_str("\nLoaded infrastructure only. Visit this territory to inspect its buildings. Upgrade checks the complete world.");}
                if let Some(cost)=r.next {
                    text.push_str(&format!("\n\nNext: {}\n{}\nTreasury {} / {} cr\nCommunity XP {} / {}\n{}houses {} / {} · farms {} / {}",cost.tier.label(),items(&cost.materials),credits(t.treasury),credits(cost.credits),r.experience,cost.experience,if partial{"Loaded "}else{""},r.houses,cost.houses,r.farms,cost.farms));
                } else { text.push_str("\nCity charter complete. Keep services supplied."); }
            }
            text
        }).unwrap_or_else(||if ui.frontier.settlement.is_some() {"The chosen territory is no longer in this view. Choose Next territory to review an available destination before travelling, contributing or funding.".into()}else{"No territory selected. Found a named neighborhood at your active actor's location, then grow it into a village and city. Claims govern building rights and do not change real political borders.".into()}),
        FrontierLabel::Contribution => {let item=CONTRIBUTIONS[ui.frontier.contribution % CONTRIBUTIONS.len()];format!("Contribute: {} · own {}\n{} still required · change",item.replace('_'," "),required_inventory(s,item),territory(s,ui).map_or(0,|t|contribution_remaining(t,item)))},
    }
}

pub(crate) fn move_to(
    s: &mut SandboxRuntime,
    target: GeoPosition,
    service: bool,
    resident: &ResidentChunks,
    runtime: &WorldRuntime,
    control: &mut PlayerControl,
    queue: &mut ControlQueue,
) -> Option<Command> {
    let active = s.state.living.active_unit(s.player_id)?;
    let unit = active.id;
    let origin = active.position;
    let mut candidate_body = active.clone();
    let weather = runtime.effective_weather();
    let actor_radius = s
        .state
        .transport
        .vehicles
        .get(&unit)
        .map(|v| {
            let dimensions = v.model.dimensions_m();
            dimensions[0].hypot(dimensions[2]) * 0.5
        })
        .unwrap_or(if active.kind == terra_core::ActorKind::Vehicle {
            3.
        } else {
            0.8
        });
    let mut obstacles: Vec<_> = s
        .state
        .structures
        .values()
        .map(|v| (v.position, v.radius_m))
        .collect();
    obstacles.extend(s.state.living.facility_footprints());
    let footprint = obstacles
        .iter()
        .filter(|(position, _)| position.surface_distance_m(target) < 1.)
        .map(|(_, radius)| *radius)
        .fold(0_f64, f64::max);
    let standoff = (footprint + actor_radius + 1.5).max(if service { 8. } else { 2. });
    if standoff > 18. {
        s.last_message = "This actor cannot safely stop within interaction reach. Select a person or approach a clear side of the destination.".into();
        return None;
    }
    let frame = LocalFrame::new(target);
    let mut candidates: Vec<_> = (0..8)
        .flat_map(|index| {
            let angle = index as f64 * std::f64::consts::FRAC_PI_4;
            [standoff, (standoff + 3.).min(18.)]
                .map(move |radius| [angle.cos() * radius, 0., angle.sin() * radius])
        })
        .filter_map(|offset| frame.to_geo(offset).ok())
        .filter_map(|mut p| {
            p.altitude_m = terra_bevy::environment::physical_surface(resident, p, &weather)?
                .terrain_height_m
                + 0.025;
            Some(p)
        })
        .collect();
    candidates.sort_by(|a, b| distance(origin, *a).total_cmp(&distance(origin, *b)));
    let destination = candidates.into_iter().find(|p| {
        candidate_body.position = *p;
        if candidate_body.kind == terra_core::ActorKind::Vehicle {
            let Some(vehicle) = s.state.transport.vehicles.get(&unit) else {
                return false;
            };
            let Some(config) = s.state.transport.vehicle_dynamics_config(unit) else {
                return false;
            };
            let plan = s.state.living.vehicle_route_plan(origin, *p);
            if plan.is_blocked() {
                return false;
            }
            // Use the final planned approach direction, rather than the
            // actor's heading before it begins the trip. Wheel support and the
            // full body envelope share native driving's exact ground policy.
            let previous = plan
                .points
                .iter()
                .rev()
                .find(|point| point.surface_distance_m(*p) > 0.001)
                .copied()
                .unwrap_or(origin);
            let delta = LocalFrame::new(previous).to_local(*p);
            let heading = if delta[0].hypot(delta[2]) > 0.001 {
                delta[0].atan2(-delta[2]).to_degrees().rem_euclid(360.)
            } else {
                f64::from(candidate_body.heading_deg)
            };
            candidate_body.heading_deg = heading as f32;
            let dimensions = vehicle.model.dimensions_m();
            if terra_bevy::environment::vehicle_ground_support(
                resident,
                *p,
                heading,
                config.wheelbase_m,
                dimensions[0],
                &weather,
            )
            .is_none()
                || !terra_core::vehicle_support_points(*p, heading, dimensions).is_ok_and(
                    |points| {
                        points.into_iter().all(|point| {
                            terra_bevy::environment::vehicle_ground_sample(
                                resident, point, &weather,
                            )
                            .is_some()
                                && !terra_bevy::environment::source_building_at_with_clearance(
                                    resident,
                                    point,
                                    0.2,
                                    dimensions[1],
                                )
                        })
                    },
                )
            {
                return false;
            }
        }
        distance(*p, target) <= terra_bevy::rules::INTERACTION_REACH_M
            && obstacles
                .iter()
                .all(|(center, radius)| center.surface_distance_m(*p) > radius + actor_radius)
            && !terra_bevy::environment::source_building_at(resident, *p, actor_radius)
            && s.state
                .spawn_obstruction(&candidate_body)
                .is_ok_and(|other| other.is_none())
            && terra_bevy::sandbox::placement_error(
                &Command::ClaimPlot {
                    center: *p,
                    radius_m: 5.,
                },
                s,
                runtime,
                resident,
            )
            .is_none()
    });
    let Some(destination) = destination else {
        s.last_message="No supported, unoccupied approach is loaded within interaction reach. Explore the destination, move blocking actors, or select a person and try again.".into();
        return None;
    };
    queue.0.push_back(ControlIntent::Release);
    control.last_order = Some(destination);
    control.selected.clear();
    control.selected.insert(unit);
    Some(Command::Living {
        action: LivingCommand::MoveUnits {
            unit_ids: vec![unit],
            destination,
        },
    })
}
pub fn act(
    action: FrontierAction,
    ui: &mut UiState,
    s: &mut SandboxRuntime,
    resident: &ResidentChunks,
    runtime: &WorldRuntime,
    control: &mut PlayerControl,
    queue: &mut ControlQueue,
) -> Option<Command> {
    let wrap = |action| Some(Command::Settlement { action });
    match action {
        FrontierAction::ChooseJourney => {
            ui.frontier.journey_choices_open = !ui.frontier.journey_choices_open;
            None
        }
        FrontierAction::SelectJourney(track) => {
            ui.frontier.guide_track = track;
            ui.frontier.journey_choices_open = false;
            ui.frontier.guide = None;
            ui.frontier.guide_context = None;
            ui.frontier.scroll_revision = ui.frontier.scroll_revision.wrapping_add(1);
            s.last_message = format!(
                "Goal selected: {}. Review your next step in Activities.",
                track.label()
            );
            None
        }
        FrontierAction::ReviewAcceptedSupply => {
            let shown = next_accepted_supply(ui).cloned();
            let unchanged = ui.frontier.commitments_context
                == Some((s.player_id, s.restore_revision))
                && shown.as_ref().is_some_and(|request| {
                    request.claimed_by == Some(s.player_id)
                        && s.state.settlements.contracts.get(&request.id) == Some(request)
                });
            if !unchanged {
                cache_guidance(s, ui);
                ui.frontier.guide = None;
                ui.frontier.commitments_context = None;
                s.last_message =
                    "Your accepted work changed. Review the updated request before opening it."
                        .into();
                return None;
            }
            let request_id = shown?.id;
            ui.frontier.contract = Some(request_id);
            ui.frontier.guide_track = GuideTrack::Supply;
            ui.frontier.journey_choices_open = false;
            ui.frontier.guide = None;
            ui.frontier.guide_context = None;
            ui.frontier.commitments_context = None;
            ui.frontier.scroll_revision = ui.frontier.scroll_revision.wrapping_add(1);
            s.last_message = format!(
                "Reviewing supply request #{request_id}. Choose its next action when ready."
            );
            // Opening a request never accepts, cancels, travels or delivers.
            None
        }
        FrontierAction::ContinueJourney => {
            let guide = s.state.activity_guidance(
                s.player_id,
                &s.catalog,
                ui.frontier.guide_track,
                ui.frontier.contract,
            );
            let unchanged = ui.frontier.guide_context == Some((s.player_id, s.restore_revision))
                && ui.frontier.guide_actor == active_identity(s)
                && ui
                    .frontier
                    .guide
                    .as_ref()
                    .is_some_and(|shown| shown.same_intent(&guide));
            if !unchanged {
                ui.frontier.guide = Some(guide);
                ui.frontier.guide_context = Some((s.player_id, s.restore_revision));
                ui.frontier.guide_actor = active_identity(s);
                s.last_message = "Your next step changed with the world. Review the updated action before choosing it.".into();
                return None;
            }
            ui.frontier.guide = None;
            ui.frontier.guide_context = None;
            match guide.action? {
                GuideAction::Command(command) => Some(command),
                GuideAction::Travel { position, service } => {
                    move_to(s, position, service, resident, runtime, control, queue)
                }
                GuideAction::Open(surface) => {
                    if surface == GuideSurface::Command
                        && let Some(person) = s.state.living.active_unit(s.player_id)
                        && person.is_person()
                        && person.owner == Some(s.player_id)
                    {
                        control.selected.clear();
                        control.selected.insert(person.id);
                    }
                    ui.panel = match surface {
                        GuideSurface::Command => Panel::Command,
                        GuideSurface::Nature => Panel::Nature,
                        GuideSurface::Build => Panel::Build,
                        GuideSurface::Market => Panel::Market,
                        GuideSurface::Territory => Panel::Territory,
                        GuideSurface::Driving => Panel::Driving,
                        GuideSurface::Garage => Panel::Garage,
                        GuideSurface::Fleet => Panel::Fleet,
                    };
                    ui.context_panel = true;
                    ui.panel_navigation_open = false;
                    None
                }
                GuideAction::OpenMarket { shop_id } => {
                    ui.market.selected_shop = Some(shop_id);
                    ui.panel = Panel::Market;
                    ui.context_panel = true;
                    ui.panel_navigation_open = false;
                    None
                }
                GuideAction::OpenDriving { job_id, vehicle_id } => {
                    ui.driving.job = Some(job_id);
                    ui.driving.vehicle = Some(vehicle_id);
                    ui.driving.route_to_garage = false;
                    ui.driving.route_to_course = false;
                    ui.panel = Panel::Driving;
                    ui.context_panel = true;
                    ui.panel_navigation_open = false;
                    None
                }
                GuideAction::OpenCompany(focus) => {
                    use crate::fleet::{FleetAction, FleetTab};
                    let tab = match focus.section {
                        GuideCompanySection::Company => FleetTab::Company,
                        GuideCompanySection::Drivers | GuideCompanySection::Garage => {
                            FleetTab::Drivers
                        }
                        GuideCompanySection::Routes => FleetTab::Routes,
                        GuideCompanySection::Recovery => FleetTab::Recovery,
                        GuideCompanySection::Depot => FleetTab::Depot,
                        GuideCompanySection::Finances => FleetTab::Finances,
                    };
                    // The normal navigation action also discards an unrelated
                    // confirmation; opening the guide must not inherit it.
                    let _ = crate::fleet::act(FleetAction::Tab(tab), ui, s);
                    ui.fleet.driver = focus.driver_id;
                    ui.fleet.route = focus.route_id;
                    ui.fleet.vehicle = focus.vehicle_id;
                    ui.fleet.stop = focus.facility_id;
                    ui.fleet.relief = None;
                    if focus.section == GuideCompanySection::Depot {
                        ui.fleet.depot = focus.facility_id;
                    }
                    ui.panel = if focus.section == GuideCompanySection::Garage {
                        ui.driving.vehicle = focus.vehicle_id;
                        ui.driving.garage = focus.facility_id;
                        ui.driving.sell_confirm = false;
                        ui.driving.rental_return_confirm = false;
                        ui.driving.route_to_garage = false;
                        ui.driving.route_to_course = false;
                        Panel::Garage
                    } else {
                        Panel::Fleet
                    };
                    ui.context_panel = true;
                    ui.panel_navigation_open = false;
                    None
                }
                GuideAction::Choose(track) => {
                    ui.frontier.guide_track = track;
                    None
                }
                GuideAction::NameNeighborhood => {
                    ui.frontier.faction_claim = false;
                    ui.input_mode = InputMode::SettlementName;
                    ui.gps_text.clear();
                    ui.editing_gps = true;
                    None
                }
            }
        }
        FrontierAction::NextContract => {
            let ids: Vec<_> = s
                .state
                .settlements
                .contracts
                .values()
                .filter(|c| c.claimed_by.is_none() || c.claimed_by == Some(s.player_id))
                .map(|c| c.id)
                .collect();
            ui.frontier.contract = next(&ids, contract(s, ui).map(|c| c.id));
            ui.frontier.guide = None;
            ui.frontier.guide_context = None;
            // The next accepted-review label depends on this selection. A
            // second queued click must wait until that new label is displayed.
            ui.frontier.commitments_context = None;
            None
        }
        FrontierAction::AcceptContract => wrap(SettlementCommand::AcceptContract {
            contract_id: ready_reviewed_contract(s, ui, SupplyContractAction::Accept)?.id,
        }),
        FrontierAction::CompleteContract => wrap(SettlementCommand::FulfillContract {
            contract_id: ready_reviewed_contract(s, ui, SupplyContractAction::Deliver)?.id,
        }),
        FrontierAction::CancelContract => wrap(SettlementCommand::CancelContract {
            contract_id: ready_reviewed_contract(s, ui, SupplyContractAction::Return)?.id,
        }),
        FrontierAction::GoContract => {
            let target = reviewed_contract(s, ui)?.target_position;
            move_to(s, target, true, resident, runtime, control, queue)
        }
        FrontierAction::NextResourceKind => {
            ui.frontier.resource_kind = (ui.frontier.resource_kind + 1) % RESOURCE_ITEMS.len();
            ui.frontier.resource = None;
            None
        }
        FrontierAction::NextResource => {
            let item = RESOURCE_ITEMS[ui.frontier.resource_kind].0;
            let ids: Vec<_> = s
                .state
                .resource_nodes
                .values()
                .filter(|r| r.item == item)
                .map(|r| r.id.clone())
                .collect();
            ui.frontier.resource = next(&ids, resource(s, ui).map(|r| r.id.clone()));
            None
        }
        FrontierAction::GoResource => {
            let target = resource(s, ui)?.position;
            move_to(s, target, false, resident, runtime, control, queue)
        }
        FrontierAction::GatherResource => {
            let r = resource(s, ui)?;
            Some(Command::Gather {
                node_id: r.id.clone(),
                amount: r.remaining.min(5),
            })
        }
        FrontierAction::EatBerries => Some(Command::Living {
            action: LivingCommand::Consume {
                item: "berries".into(),
            },
        }),
        FrontierAction::EatFish => Some(Command::Living {
            action: LivingCommand::Consume {
                item: "cooked_fish".into(),
            },
        }),
        FrontierAction::DrinkWater => Some(Command::Living {
            action: LivingCommand::Consume {
                item: "water".into(),
            },
        }),
        FrontierAction::CraftRod
        | FrontierAction::CookFish
        | FrontierAction::SmeltIron
        | FrontierAction::CraftIronTool => Some(Command::Craft {
            recipe: match action {
                FrontierAction::CraftRod => "fishing_rod",
                FrontierAction::CookFish => "cooked_fish",
                FrontierAction::SmeltIron => "iron_ingot",
                _ => "iron_tool",
            }
            .into(),
            count: 1,
        }),
        FrontierAction::WatchAnimal => {
            let ids: Vec<_> = s
                .state
                .ecology
                .animals
                .keys()
                .filter(|id| s.state.living.units.contains_key(id))
                .copied()
                .collect();
            let id = next(&ids, ui.frontier.animal)?;
            ui.frontier.animal = Some(id);
            control.selected.clear();
            control.selected.insert(id);
            queue.0.push_back(ControlIntent::Release);
            queue.0.push_back(ControlIntent::FocusSelection);
            s.last_message =
                "Observing wildlife. Keep your distance to watch undisturbed behavior.".into();
            None
        }
        FrontierAction::Found => {
            ui.input_mode = InputMode::SettlementName;
            ui.gps_text.clear();
            ui.editing_gps = true;
            None
        }
        FrontierAction::ClaimOwner => {
            ui.frontier.faction_claim = !ui.frontier.faction_claim;
            None
        }
        FrontierAction::NextSettlement => {
            let ids: Vec<_> = s.state.settlements.territories.keys().copied().collect();
            ui.frontier.settlement = next(&ids, territory(s, ui).map(|t| t.id));
            None
        }
        FrontierAction::GoSettlement => {
            let target = territory(s, ui)?.center;
            move_to(s, target, false, resident, runtime, control, queue)
        }
        FrontierAction::NextContribution => {
            ui.frontier.contribution = (ui.frontier.contribution + 1) % CONTRIBUTIONS.len();
            None
        }
        FrontierAction::Contribute => contribute(s, ui),
        FrontierAction::Fund => wrap(SettlementCommand::Fund {
            settlement_id: territory(s, ui)?.id,
            amount: 2500,
        }),
        FrontierAction::Upgrade => wrap(SettlementCommand::Upgrade {
            settlement_id: territory(s, ui)?.id,
        }),
    }
}
fn next<T: Clone + PartialEq>(ids: &[T], current: Option<T>) -> Option<T> {
    if ids.is_empty() {
        None
    } else {
        let index = current
            .and_then(|x| ids.iter().position(|id| *id == x))
            .map_or(0, |i| (i + 1) % ids.len());
        Some(ids[index].clone())
    }
}
pub fn unavailable(
    action: FrontierAction,
    s: &SandboxRuntime,
    ui: &UiState,
    partial: bool,
) -> bool {
    let here = position(s);
    let partial = partial || s.state.is_partial();
    if !s.state.living.enabled || here.is_none() || s.state.is_player_dormant(s.player_id) {
        return true;
    }
    let far = |p| here.is_none_or(|h| distance(h, p) > 20.);
    let active = s.state.living.active_unit(s.player_id);
    match action {
        FrontierAction::ChooseJourney | FrontierAction::SelectJourney(_) => false,
        FrontierAction::ReviewAcceptedSupply => next_accepted_supply(ui).is_none(),
        FrontierAction::ContinueJourney => ui
            .frontier
            .guide
            .as_ref()
            .is_none_or(|g| g.action.is_none()),
        FrontierAction::AcceptContract
        | FrontierAction::CompleteContract
        | FrontierAction::CancelContract => contract(s, ui).is_none_or(|contract| {
            let action = match action {
                FrontierAction::AcceptContract => SupplyContractAction::Accept,
                FrontierAction::CompleteContract => SupplyContractAction::Deliver,
                _ => SupplyContractAction::Return,
            };
            s.state
                .supply_contract_readiness(s.player_id, contract.id, action, &s.catalog)
                .is_err()
        }),
        FrontierAction::GoContract | FrontierAction::NextContract => contract(s, ui).is_none(),
        FrontierAction::GatherResource => resource_blocker(s, ui).is_some(),
        FrontierAction::GoResource => resource(s, ui).is_none(),
        FrontierAction::NextResource => !s.state.resource_nodes.values().any(|node| {
            node.item == RESOURCE_ITEMS[ui.frontier.resource_kind % RESOURCE_ITEMS.len()].0
        }),
        FrontierAction::EatBerries => {
            required_inventory(s, "berries") == 0
                || active.is_none_or(|u| !u.is_person() || u.needs.food >= 0.98)
        }
        FrontierAction::EatFish => {
            required_inventory(s, "cooked_fish") == 0
                || active.is_none_or(|u| !u.is_person() || u.needs.food >= 0.98)
        }
        FrontierAction::DrinkWater => {
            required_inventory(s, "water") == 0
                || active.is_none_or(|u| !u.is_person() || u.needs.water >= 0.98)
        }
        FrontierAction::WatchAnimal => s.state.ecology.animals.is_empty(),
        FrontierAction::Contribute | FrontierAction::Fund | FrontierAction::Upgrade => {
            territory(s, ui).is_none_or(|t| {
                if !s.state.may_build(s.player_id, t.faction_owner, t.founder) || far(t.center) {
                    return true;
                }
                match action {
                    FrontierAction::Contribute => {
                        required_inventory(s, CONTRIBUTIONS[ui.frontier.contribution]) == 0
                            || contribution_remaining(t, CONTRIBUTIONS[ui.frontier.contribution])
                                == 0
                    }
                    FrontierAction::Fund => s
                        .state
                        .living
                        .accounts
                        .get(&s.player_id)
                        .is_none_or(|a| a.money < 2500),
                    _ => s
                        .state
                        .settlement_readiness(s.player_id, t.id)
                        .ok()
                        .is_none_or(|r| {
                            r.next.is_none_or(|c| {
                                r.experience < c.experience
                                    || (!partial && (r.houses < c.houses || r.farms < c.farms))
                                    || t.treasury < c.credits
                                    || c.materials.iter().any(|(item, n)| {
                                        t.stock.get(item).copied().unwrap_or(0) < *n
                                    })
                            })
                        }),
                }
            })
        }
        FrontierAction::GoSettlement => territory(s, ui).is_none(),
        FrontierAction::NextSettlement => s.state.settlements.territories.is_empty(),
        FrontierAction::Found => ui.frontier.faction_claim && ui.selected_faction.is_none(),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::BTreeMap, sync::Arc, time::Duration};
    use terra_core::{
        FeatureKind, HeightField, TileKey, WeatherSample, WorldChunk, WorldClock, WorldFeature,
    };

    fn guidance_runtime() -> (SandboxRuntime, GeoPosition, Vec<u64>) {
        let origin = GeoPosition::new(48.137, 11.575, 100.).unwrap();
        let mut s = SandboxRuntime::new(origin);
        s.state.seed_living_demo(origin, &[], &s.catalog).unwrap();
        s.state.advance_settlements(0., &s.catalog).unwrap();
        let ids = s
            .state
            .settlements
            .contracts
            .keys()
            .copied()
            .take(3)
            .collect();
        (s, origin, ids)
    }

    fn accepted_request(s: &mut SandboxRuntime, id: u64) {
        s.state
            .apply(
                s.player_id,
                Command::Settlement {
                    action: SettlementCommand::AcceptContract { contract_id: id },
                },
                &s.catalog,
            )
            .unwrap();
    }

    fn reviewed_token(ui: &UiState, action: FrontierAction) -> u64 {
        let Some(Action::FrontierReviewed { token, .. }) = bind_action(ui, action) else {
            panic!("expected a displayed frontier action")
        };
        token
    }

    fn reviewed_territories(s: &mut SandboxRuntime, origin: GeoPosition) -> Vec<u64> {
        s.state
            .players
            .get_mut(&s.player_id)
            .unwrap()
            .inventory
            .insert("wood".into(), 40);
        let mut ids = Vec::new();
        for (name, offset) in [("First neighborhood", 0.), ("Second neighborhood", 400.)] {
            let center = LocalFrame::new(origin).to_geo([offset, 0., 0.]).unwrap();
            s.state.set_player_position(s.player_id, center).unwrap();
            ids.push(
                s.state
                    .apply(
                        s.player_id,
                        Command::Settlement {
                            action: SettlementCommand::Found {
                                name: name.into(),
                                center,
                                faction_owner: None,
                            },
                        },
                        &s.catalog,
                    )
                    .unwrap()
                    .created_id
                    .unwrap(),
            );
        }
        s.state.set_player_position(s.player_id, origin).unwrap();
        s.state
            .players
            .get_mut(&s.player_id)
            .unwrap()
            .inventory
            .insert("planks".into(), 10);
        ids
    }

    #[test]
    fn reviewed_resource_actions_reject_missing_replaced_and_newly_selected_targets() {
        for action in [FrontierAction::GoResource, FrontierAction::GatherResource] {
            for changed in 0..4 {
                let (mut s, origin, _) = guidance_runtime();
                s.state
                    .seed_resource("review_patch_a".into(), origin, "berries", 10, 0.)
                    .unwrap();
                s.state
                    .seed_resource("review_patch_b".into(), origin, "berries", 10, 0.)
                    .unwrap();
                let mut ui = UiState::default();
                ui.frontier.resource = Some("review_patch_a".into());
                refresh_displayed_actions(&s, &mut ui);
                let token = reviewed_token(&ui, action);
                match changed {
                    0 => ui.frontier.resource = Some("review_patch_b".into()),
                    1 => {
                        s.state.resource_nodes.remove("review_patch_a");
                    }
                    2 => {
                        s.state
                            .resource_nodes
                            .get_mut("review_patch_a")
                            .unwrap()
                            .position
                            .latitude += 0.001
                    }
                    _ => s.restore_revision += 1,
                }
                let before = s.state.clone();
                let mut controls = ControlQueue::default();
                assert!(
                    act_reviewed(
                        action,
                        token,
                        &mut ui,
                        &mut s,
                        &ResidentChunks::default(),
                        &runtime(origin),
                        &mut PlayerControl::default(),
                        &mut controls
                    )
                    .is_none()
                );
                assert_eq!(s.state, before);
                assert!(controls.0.is_empty());
                assert!(s.last_message.contains("Review"));
                if changed == 1 {
                    assert!(
                        resource(&s, &ui).is_none(),
                        "missing explicit patch cannot borrow another patch"
                    );
                    assert!(
                        !unavailable(FrontierAction::NextResource, &s, &ui, false),
                        "choosing a new patch remains possible"
                    );
                }
            }
        }
    }

    #[test]
    fn reviewed_territory_actions_reject_selection_value_and_restore_changes() {
        for action in [
            FrontierAction::GoSettlement,
            FrontierAction::Contribute,
            FrontierAction::Fund,
            FrontierAction::Upgrade,
        ] {
            for changed in 0..5 {
                let (mut s, origin, _) = guidance_runtime();
                let ids = reviewed_territories(&mut s, origin);
                let mut ui = UiState::default();
                ui.frontier.settlement = Some(ids[0]);
                refresh_displayed_actions(&s, &mut ui);
                let token = reviewed_token(&ui, action);
                match changed {
                    0 => ui.frontier.settlement = Some(ids[1]),
                    1 => {
                        s.state.settlements.territories.remove(&ids[0]);
                    }
                    2 => {
                        s.state
                            .settlements
                            .territories
                            .get_mut(&ids[0])
                            .unwrap()
                            .treasury += 1
                    }
                    3 => ui.frontier.contribution = 1,
                    _ => s.restore_revision += 1,
                }
                let before = s.state.clone();
                let mut controls = ControlQueue::default();
                assert!(
                    act_reviewed(
                        action,
                        token,
                        &mut ui,
                        &mut s,
                        &ResidentChunks::default(),
                        &runtime(origin),
                        &mut PlayerControl::default(),
                        &mut controls
                    )
                    .is_none()
                );
                assert_eq!(s.state, before);
                assert!(controls.0.is_empty());
                if changed == 1 {
                    assert!(territory(&s, &ui).is_none());
                    assert!(!unavailable(FrontierAction::NextSettlement, &s, &ui, false));
                }
            }
        }
    }

    #[test]
    fn refreshed_frontier_display_never_retargets_a_queued_token_and_unchanged_actions_execute() {
        let (mut s, origin, _) = guidance_runtime();
        s.state
            .seed_resource("review_patch_a".into(), origin, "berries", 10, 0.)
            .unwrap();
        s.state
            .seed_resource("review_patch_b".into(), origin, "berries", 10, 0.)
            .unwrap();
        let mut ui = UiState::default();
        ui.frontier.resource = Some("review_patch_a".into());
        refresh_displayed_actions(&s, &mut ui);
        let old = reviewed_token(&ui, FrontierAction::GatherResource);
        ui.frontier.resource = Some("review_patch_b".into());
        refresh_displayed_actions(&s, &mut ui);
        ui.frontier.resource = Some("review_patch_a".into());
        refresh_displayed_actions(&s, &mut ui);
        let mut controls = ControlQueue::default();
        let mut control = PlayerControl::default();
        assert!(
            act_reviewed(
                FrontierAction::GatherResource,
                old,
                &mut ui,
                &mut s,
                &ResidentChunks::default(),
                &runtime(origin),
                &mut control,
                &mut controls
            )
            .is_none()
        );
        let token = reviewed_token(&ui, FrontierAction::GatherResource);
        let command = act_reviewed(
            FrontierAction::GatherResource,
            token,
            &mut ui,
            &mut s,
            &ResidentChunks::default(),
            &runtime(origin),
            &mut control,
            &mut controls,
        )
        .unwrap();
        let before = required_inventory(&s, "berries");
        s.state.apply(s.player_id, command, &s.catalog).unwrap();
        assert_eq!(required_inventory(&s, "berries"), before + 5);
        assert_eq!(s.state.resource_nodes["review_patch_a"].remaining, 5);
        assert_eq!(s.state.resource_nodes["review_patch_b"].remaining, 10);

        let ids = reviewed_territories(&mut s, origin);
        ui.frontier.settlement = Some(ids[0]);
        refresh_displayed_actions(&s, &mut ui);
        let wallet = s.state.living.accounts[&s.player_id].money;
        let treasury = s.state.settlements.territories[&ids[0]].treasury;
        let other = s.state.settlements.territories[&ids[1]].clone();
        let token = reviewed_token(&ui, FrontierAction::Fund);
        let command = act_reviewed(
            FrontierAction::Fund,
            token,
            &mut ui,
            &mut s,
            &ResidentChunks::default(),
            &runtime(origin),
            &mut control,
            &mut controls,
        )
        .unwrap();
        s.state.apply(s.player_id, command, &s.catalog).unwrap();
        assert_eq!(s.state.living.accounts[&s.player_id].money, wallet - 2500);
        assert_eq!(
            s.state.settlements.territories[&ids[0]].treasury,
            treasury + 2500
        );
        assert_eq!(s.state.settlements.territories[&ids[1]], other);
    }

    #[test]
    fn native_supply_readiness_explains_wallet_capacity_and_recovers_without_losing_goods() {
        let (mut s, origin, ids) = guidance_runtime();
        accepted_request(&mut s, ids[0]);
        let request = s.state.settlements.contracts[&ids[0]].clone();
        s.state
            .players
            .get_mut(&s.player_id)
            .unwrap()
            .inventory
            .insert(request.item.clone(), request.amount);
        s.state
            .set_player_position(s.player_id, request.target_position)
            .unwrap();
        let wallet = s.state.living.accounts[&s.player_id].money;
        s.state.living.accounts.get_mut(&s.player_id).unwrap().money = terra_bevy::rules::MAX_MONEY;
        let mut ui = UiState::default();
        ui.frontier.contract = Some(request.id);
        cache_guidance(&s, &mut ui);
        refresh_displayed_actions(&s, &mut ui);
        assert!(unavailable(
            FrontierAction::CompleteContract,
            &s,
            &ui,
            false
        ));
        let explanation = label(FrontierLabel::ContractReadiness, &s, &ui, false, false);
        assert!(explanation.contains("wallet") && explanation.contains("Spend"));
        assert!(!unavailable(FrontierAction::CancelContract, &s, &ui, false));
        let before = s.state.clone();
        let token = reviewed_token(&ui, FrontierAction::CompleteContract);
        assert!(
            act_reviewed(
                FrontierAction::CompleteContract,
                token,
                &mut ui,
                &mut s,
                &ResidentChunks::default(),
                &runtime(origin),
                &mut PlayerControl::default(),
                &mut ControlQueue::default()
            )
            .is_none()
        );
        assert_eq!(s.state, before);

        // The fixture removes the capacity condition; normal authority performs
        // the same exact escrow-to-wallet and goods-to-receiver exchange.
        s.state.living.accounts.get_mut(&s.player_id).unwrap().money = wallet;
        cache_guidance(&s, &mut ui);
        refresh_displayed_actions(&s, &mut ui);
        assert!(!unavailable(
            FrontierAction::CompleteContract,
            &s,
            &ui,
            false
        ));
        assert!(
            label(FrontierLabel::ContractReadiness, &s, &ui, false, false)
                .starts_with("Ready to deliver")
        );
        let token = reviewed_token(&ui, FrontierAction::CompleteContract);
        let command = act_reviewed(
            FrontierAction::CompleteContract,
            token,
            &mut ui,
            &mut s,
            &ResidentChunks::default(),
            &runtime(origin),
            &mut PlayerControl::default(),
            &mut ControlQueue::default(),
        )
        .unwrap();
        s.state.apply(s.player_id, command, &s.catalog).unwrap();
        assert_eq!(
            s.state.living.accounts[&s.player_id].money,
            wallet + request.reward
        );
        assert_eq!(required_inventory(&s, &request.item), 0);
        assert!(!s.state.settlements.contracts.contains_key(&request.id));
    }

    #[test]
    fn native_noticeboard_never_advertises_expired_acceptance_and_names_current_recovery() {
        let (mut s, origin, ids) = guidance_runtime();
        let id = ids[0];
        s.state
            .settlements
            .contracts
            .get_mut(&id)
            .unwrap()
            .expires_at = s.state.settlements.elapsed_seconds;
        let mut ui = UiState::default();
        ui.frontier.contract = Some(id);
        cache_guidance(&s, &mut ui);
        refresh_displayed_actions(&s, &mut ui);
        assert!(unavailable(FrontierAction::AcceptContract, &s, &ui, false));
        assert!(label(FrontierLabel::ContractReadiness, &s, &ui, false, false).contains("expired"));
        let before = s.state.clone();
        let token = reviewed_token(&ui, FrontierAction::AcceptContract);
        assert!(
            act_reviewed(
                FrontierAction::AcceptContract,
                token,
                &mut ui,
                &mut s,
                &ResidentChunks::default(),
                &runtime(origin),
                &mut PlayerControl::default(),
                &mut ControlQueue::default()
            )
            .is_none()
        );
        assert_eq!(s.state, before);
        assert!(s.last_message.contains("expired"));
    }

    #[test]
    fn direct_goal_choices_are_read_only_and_the_chooser_follows_visible_state() {
        let (mut s, origin, ids) = guidance_runtime();
        accepted_request(&mut s, ids[0]);
        let before = s.state.clone();
        let mut ui = UiState::default();
        let mut control = PlayerControl::default();
        let mut queue = ControlQueue::default();
        for track in [
            GuideTrack::Home,
            GuideTrack::Company,
            GuideTrack::Supply,
            GuideTrack::Community,
            GuideTrack::Road,
            GuideTrack::Suggested,
        ] {
            ui.frontier.journey_choices_open = true;
            let scroll_revision = ui.frontier.scroll_revision;
            assert!(
                act(
                    FrontierAction::SelectJourney(track),
                    &mut ui,
                    &mut s,
                    &ResidentChunks::default(),
                    &runtime(origin),
                    &mut control,
                    &mut queue
                )
                .is_none()
            );
            assert_eq!(ui.frontier.guide_track, track);
            assert!(!ui.frontier.journey_choices_open);
            assert_eq!(ui.frontier.scroll_revision, scroll_revision + 1);
            assert!(s.last_message.contains(track.label()));
            let scroll_revision = ui.frontier.scroll_revision;
            cache_guidance(&s, &mut ui);
            assert_eq!(
                ui.frontier.scroll_revision, scroll_revision,
                "automatic guidance refresh preserves the reader's place"
            );
            assert_eq!(
                ui.frontier.commitments.as_ref().unwrap().supply_requests[0].id,
                ids[0]
            );
        }
        assert_eq!(s.state, before);
        assert!(queue.0.is_empty());

        let mut app = App::new();
        app.insert_resource(s)
            .insert_resource(ui)
            .init_resource::<Time<Real>>()
            .add_systems(Update, refresh_guidance);
        let chooser = app
            .world_mut()
            .spawn((JourneyChoices, Node::default()))
            .id();
        app.update();
        assert_eq!(
            app.world().get::<Node>(chooser).unwrap().display,
            Display::None
        );
        app.world_mut()
            .resource_mut::<UiState>()
            .frontier
            .journey_choices_open = true;
        app.update();
        assert_eq!(
            app.world().get::<Node>(chooser).unwrap().display,
            Display::Flex
        );
        app.world_mut()
            .resource_mut::<UiState>()
            .frontier
            .journey_choices_open = false;
        app.update();
        assert_eq!(
            app.world().get::<Node>(chooser).unwrap().display,
            Display::None
        );
    }

    #[test]
    fn commitment_review_selects_the_displayed_request_without_performing_it() {
        let (mut s, origin, ids) = guidance_runtime();
        accepted_request(&mut s, ids[0]);
        accepted_request(&mut s, ids[1]);
        s.state
            .settlements
            .contracts
            .get_mut(&ids[0])
            .unwrap()
            .expires_at = 9000.;
        s.state
            .settlements
            .contracts
            .get_mut(&ids[1])
            .unwrap()
            .expires_at = 4000.;
        let mut ui = UiState::default();
        ui.frontier.contract = Some(ids[2]);
        cache_guidance(&s, &mut ui);
        let before = s.state.clone();
        let mut control = PlayerControl::default();
        let mut queue = ControlQueue::default();
        assert!(
            label(FrontierLabel::ReviewAcceptedSupply, &s, &ui, false, false)
                .contains(&ids[1].to_string())
        );
        assert!(
            act(
                FrontierAction::ReviewAcceptedSupply,
                &mut ui,
                &mut s,
                &ResidentChunks::default(),
                &runtime(origin),
                &mut control,
                &mut queue
            )
            .is_none()
        );
        assert_eq!(ui.frontier.contract, Some(ids[1]));
        assert_eq!(ui.frontier.guide_track, GuideTrack::Supply);
        assert_eq!(ui.frontier.scroll_revision, 1);
        assert!(
            s.last_message
                .contains(&format!("Reviewing supply request #{}", ids[1]))
        );
        assert!(
            ui.frontier.guide.is_none(),
            "new action must first be shown"
        );
        cache_guidance(&s, &mut ui);
        assert_eq!(ui.frontier.scroll_revision, 1);
        assert_eq!(
            ui.frontier
                .guide
                .as_ref()
                .unwrap()
                .supply_contract
                .as_ref()
                .unwrap()
                .id,
            ids[1]
        );
        assert!(
            label(FrontierLabel::ReviewAcceptedSupply, &s, &ui, false, false)
                .contains(&ids[0].to_string())
        );
        assert_eq!(s.state, before);
        assert!(queue.0.is_empty());
    }

    #[test]
    fn commitment_review_rejects_cancelled_reassigned_and_restored_context() {
        for changed in 0..3 {
            let (mut s, origin, ids) = guidance_runtime();
            accepted_request(&mut s, ids[0]);
            accepted_request(&mut s, ids[1]);
            let mut ui = UiState::default();
            cache_guidance(&s, &mut ui);
            match changed {
                0 => {
                    s.state.settlements.contracts.remove(&ids[0]);
                }
                1 => {
                    s.state
                        .settlements
                        .contracts
                        .get_mut(&ids[0])
                        .unwrap()
                        .claimed_by = None;
                }
                _ => {
                    s.restore_revision += 1;
                }
            }
            let before = s.state.clone();
            assert!(
                act(
                    FrontierAction::ReviewAcceptedSupply,
                    &mut ui,
                    &mut s,
                    &ResidentChunks::default(),
                    &runtime(origin),
                    &mut PlayerControl::default(),
                    &mut ControlQueue::default()
                )
                .is_none()
            );
            assert_eq!(
                ui.frontier.contract, None,
                "must not open a replacement request"
            );
            assert!(s.last_message.contains("Your accepted work changed"));
            assert_eq!(
                ui.frontier.scroll_revision, 0,
                "stale review does not scroll"
            );
            assert_eq!(s.state, before);
        }
    }

    #[test]
    fn queued_selection_and_review_actions_wait_for_the_new_displayed_handoff() {
        for review_first in [false, true] {
            let (mut s, origin, ids) = guidance_runtime();
            accepted_request(&mut s, ids[0]);
            if !review_first {
                accepted_request(&mut s, ids[1]);
            }
            let mut ui = UiState::default();
            ui.frontier.contract = Some(ids[0]);
            cache_guidance(&s, &mut ui);
            let before = s.state.clone();
            let mut queue = ControlQueue::default();
            let mut control = PlayerControl::default();
            let first = if review_first {
                FrontierAction::ReviewAcceptedSupply
            } else {
                FrontierAction::NextContract
            };
            assert!(
                act(
                    first,
                    &mut ui,
                    &mut s,
                    &ResidentChunks::default(),
                    &runtime(origin),
                    &mut control,
                    &mut queue
                )
                .is_none()
            );
            let selected = ui.frontier.contract;
            let scroll_revision = ui.frontier.scroll_revision;
            let second = if review_first {
                // With one accepted request, its snapshot still matches; the
                // newly opened review must nevertheless be displayed first.
                FrontierAction::CancelContract
            } else {
                FrontierAction::ReviewAcceptedSupply
            };
            assert!(
                act(
                    second,
                    &mut ui,
                    &mut s,
                    &ResidentChunks::default(),
                    &runtime(origin),
                    &mut control,
                    &mut queue
                )
                .is_none()
            );
            assert_eq!(ui.frontier.contract, selected, "no unshown retarget");
            assert_eq!(ui.frontier.scroll_revision, scroll_revision);
            assert_eq!(s.state, before);
            assert!(queue.0.is_empty());
            assert!(s.last_message.contains("changed"));
        }
    }

    #[test]
    fn noticeboard_actions_reject_changed_displayed_request_before_dispatch() {
        for action in [
            FrontierAction::AcceptContract,
            FrontierAction::CompleteContract,
            FrontierAction::CancelContract,
            FrontierAction::GoContract,
        ] {
            let (mut s, origin, ids) = guidance_runtime();
            let mut ui = UiState::default();
            ui.frontier.contract = Some(ids[0]);
            cache_guidance(&s, &mut ui);
            s.state.settlements.contracts.remove(&ids[0]);
            let before = s.state.clone();
            let mut queue = ControlQueue::default();
            assert!(
                act(
                    action,
                    &mut ui,
                    &mut s,
                    &ResidentChunks::default(),
                    &runtime(origin),
                    &mut PlayerControl::default(),
                    &mut queue
                )
                .is_none()
            );
            assert!(queue.0.is_empty());
            assert_eq!(s.state, before);
            assert!(s.last_message.contains("displayed supply request changed"));
            assert_ne!(
                ui.frontier.displayed_contract.as_ref().map(|c| c.id),
                Some(ids[0])
            );
        }
    }

    #[test]
    fn guidance_rejects_same_gather_action_for_a_different_accepted_obligation() {
        let (mut s, origin, ids) = guidance_runtime();
        for id in &ids[..2] {
            let request = s.state.settlements.contracts.get_mut(id).unwrap();
            request.item = "wood".into();
            request.amount = 10;
            // The comparison fixture changes the requested item, so align the
            // receiver's recipe with a real shortage before accepting it.
            let receiver = s
                .state
                .living
                .producers
                .get_mut(&request.target_site)
                .unwrap();
            let current = receiver.stock.get("wood").copied().unwrap_or(0);
            receiver
                .inputs
                .insert("wood".into(), (current + 10).div_ceil(12));
            accepted_request(&mut s, *id);
        }
        s.state
            .players
            .get_mut(&s.player_id)
            .unwrap()
            .inventory
            .remove("wood");
        let mut ui = UiState::default();
        ui.frontier.guide_track = GuideTrack::Supply;
        cache_guidance(&s, &mut ui);
        let shown = ui.frontier.guide.as_ref().unwrap().clone();
        s.state
            .settlements
            .contracts
            .get_mut(&ids[1])
            .unwrap()
            .expires_at = 1.;
        let current = s
            .state
            .activity_guidance(s.player_id, &s.catalog, GuideTrack::Supply, None);
        assert!(
            shown
                .action
                .as_ref()
                .unwrap()
                .same_intent(current.action.as_ref().unwrap())
        );
        let before = s.state.clone();
        let mut queue = ControlQueue::default();
        assert!(
            act(
                FrontierAction::ContinueJourney,
                &mut ui,
                &mut s,
                &ResidentChunks::default(),
                &runtime(origin),
                &mut PlayerControl::default(),
                &mut queue
            )
            .is_none()
        );
        assert!(s.last_message.contains("Your next step changed"));
        assert_eq!(s.state, before);
        assert!(queue.0.is_empty());
    }

    fn runtime(focus: GeoPosition) -> WorldRuntime {
        WorldRuntime {
            config: terra_bevy::WorldConfig {
                focus,
                ..Default::default()
            },
            focus,
            frame: LocalFrame::new(focus),
            origin_revision: 0,
            clock: WorldClock {
                time_scale: 0.,
                ..Default::default()
            },
            weather: WeatherSample::default(),
            stream_stats: Default::default(),
            last_error: None,
            stream_poll_ms: 0.,
            simulation_ms: 0.,
            rebases: 0,
            network_authority: false,
        }
    }

    fn company_command(
        s: &mut SandboxRuntime,
        action: terra_bevy::rules::CompanyCommand,
    ) -> terra_bevy::rules::Outcome {
        s.state
            .apply(
                s.player_id,
                Command::Transport {
                    action: terra_bevy::rules::TransportCommand::Company { action },
                },
                &s.catalog,
            )
            .unwrap()
    }

    fn company_runtime() -> (SandboxRuntime, GeoPosition, u64, u64) {
        use terra_bevy::rules::{CompanyCommand, FleetService, TransportFacilityKind};
        let origin = GeoPosition::new(48.137, 11.575, 100.).unwrap();
        let mut s = SandboxRuntime::new(origin);
        s.state.seed_living_demo(origin, &[], &s.catalog).unwrap();
        s.state.ensure_transport_demo(&s.catalog).unwrap();
        let garage = s
            .state
            .transport
            .facilities
            .values()
            .find(|f| f.kind == TransportFacilityKind::Garage)
            .unwrap()
            .position;
        s.state.set_player_position(s.player_id, garage).unwrap();
        let vehicle = s
            .state
            .transport
            .vehicles
            .values()
            .find(|v| v.owner == s.player_id)
            .unwrap()
            .unit_id;
        company_command(
            &mut s,
            CompanyCommand::Create {
                name: "Valley Transit".into(),
            },
        );
        company_command(&mut s, CompanyCommand::Deposit { amount: 5_000 });
        let driver = company_command(&mut s, CompanyCommand::HireDriver { name: "Ari".into() })
            .created_id
            .unwrap();
        let stops = s
            .state
            .transport
            .facilities
            .values()
            .filter(|f| f.kind == TransportFacilityKind::Terminal)
            .take(2)
            .map(|f| f.id)
            .collect();
        let route = company_command(
            &mut s,
            CompanyCommand::CreateRoute {
                name: "Town service".into(),
                stops,
                service: FleetService::Passengers { count: 1 },
            },
        )
        .created_id
        .unwrap();
        company_command(
            &mut s,
            CompanyCommand::Assign {
                driver_id: driver,
                vehicle_id: vehicle,
                route_id: route,
            },
        );
        (s, origin, driver, vehicle)
    }

    #[test]
    fn company_guide_opens_exact_recovery_and_requires_review_after_service_changes() {
        use terra_bevy::rules::CompanyCommand;
        let (mut s, origin, driver, vehicle) = company_runtime();
        company_command(&mut s, CompanyCommand::HoldVehicle { driver_id: driver });
        let mut ui = UiState::default();
        ui.panel = Panel::Activities;
        ui.frontier.guide_track = GuideTrack::Company;
        cache_guidance(&s, &mut ui);
        ui.fleet.driver = Some(999);
        ui.fleet.vehicle = Some(998);
        ui.fleet.route = Some(997);
        ui.fleet.tab = crate::fleet::FleetTab::Finances;
        let mut control = PlayerControl::default();
        let mut queue = ControlQueue::default();
        let before = s.state.clone();
        assert!(
            act(
                FrontierAction::ContinueJourney,
                &mut ui,
                &mut s,
                &ResidentChunks::default(),
                &runtime(origin),
                &mut control,
                &mut queue
            )
            .is_none()
        );
        assert_eq!(s.state, before);
        assert!(queue.0.is_empty());
        assert!(ui.panel == Panel::Fleet);
        assert!(ui.fleet.tab == crate::fleet::FleetTab::Recovery);
        assert_eq!(ui.fleet.driver, Some(driver));
        assert_eq!(ui.fleet.vehicle, Some(vehicle));
        assert_eq!(
            ui.fleet.route,
            s.state.transport.companies[&s.player_id].drivers[0].route_id
        );

        ui.panel = Panel::Activities;
        cache_guidance(&s, &mut ui);
        company_command(
            &mut s,
            CompanyCommand::SetDriverRunning {
                driver_id: driver,
                running: true,
            },
        );
        let before = s.state.clone();
        assert!(
            act(
                FrontierAction::ContinueJourney,
                &mut ui,
                &mut s,
                &ResidentChunks::default(),
                &runtime(origin),
                &mut control,
                &mut queue
            )
            .is_none()
        );
        assert!(
            ui.panel == Panel::Activities,
            "a stale action refreshes in place"
        );
        assert!(s.last_message.contains("Review the updated action"));
        assert_eq!(s.state, before);
        assert!(queue.0.is_empty());
        assert!(
            act(
                FrontierAction::ContinueJourney,
                &mut ui,
                &mut s,
                &ResidentChunks::default(),
                &runtime(origin),
                &mut control,
                &mut queue
            )
            .is_none()
        );
        assert!(ui.fleet.tab == crate::fleet::FleetTab::Drivers);
        assert_eq!(s.state, before);
    }

    #[test]
    fn company_guide_opens_exact_expired_rental_without_reusing_old_garage_confirmations() {
        use terra_bevy::rules::{
            CompanyCommand, TransportCommand, TransportFacilityKind, VehicleModel,
        };
        let (mut s, origin, driver, _) = company_runtime();
        let route = s.state.transport.companies[&s.player_id].drivers[0]
            .route_id
            .unwrap();
        company_command(&mut s, CompanyCommand::ReleaseVehicle { driver_id: driver });
        let garage = s
            .state
            .transport
            .facilities
            .values()
            .find(|f| f.kind == TransportFacilityKind::Garage)
            .unwrap()
            .id;
        let vehicle = s
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
        company_command(
            &mut s,
            CompanyCommand::Assign {
                driver_id: driver,
                vehicle_id: vehicle,
                route_id: route,
            },
        );
        s.state
            .transport
            .vehicles
            .get_mut(&vehicle)
            .unwrap()
            .rental
            .as_mut()
            .unwrap()
            .remaining_seconds = 0.;
        company_command(
            &mut s,
            CompanyCommand::SetDriverRunning {
                driver_id: driver,
                running: true,
            },
        );
        s.state.prepare_company_drivers(0.1).unwrap();
        let mut ui = UiState::default();
        ui.frontier.guide_track = GuideTrack::Company;
        cache_guidance(&s, &mut ui);
        ui.driving.vehicle = Some(999);
        ui.driving.garage = Some(998);
        ui.driving.sell_confirm = true;
        ui.driving.rental_return_confirm = true;
        let before = s.state.clone();
        let mut queue = ControlQueue::default();
        assert!(
            act(
                FrontierAction::ContinueJourney,
                &mut ui,
                &mut s,
                &ResidentChunks::default(),
                &runtime(origin),
                &mut PlayerControl::default(),
                &mut queue
            )
            .is_none()
        );
        assert!(ui.panel == Panel::Garage);
        assert_eq!(ui.driving.vehicle, Some(vehicle));
        assert_eq!(ui.driving.garage, Some(garage));
        assert!(!ui.driving.sell_confirm && !ui.driving.rental_return_confirm);
        assert_eq!(s.state, before);
        assert!(queue.0.is_empty());
    }

    #[test]
    fn guidance_requires_review_after_restore_even_when_service_ids_and_status_match() {
        let (mut s, origin, _, _) = company_runtime();
        let mut ui = UiState::default();
        ui.panel = Panel::Activities;
        ui.frontier.guide_track = GuideTrack::Company;
        cache_guidance(&s, &mut ui);
        let before = s.state.clone();
        s.restore_revision += 1;
        let mut queue = ControlQueue::default();
        assert!(
            act(
                FrontierAction::ContinueJourney,
                &mut ui,
                &mut s,
                &ResidentChunks::default(),
                &runtime(origin),
                &mut PlayerControl::default(),
                &mut queue
            )
            .is_none()
        );
        assert!(ui.panel == Panel::Activities);
        assert!(s.last_message.contains("Review the updated action"));
        assert_eq!(s.state, before);
        assert!(queue.0.is_empty());
        assert_eq!(
            ui.frontier.guide_context,
            Some((s.player_id, s.restore_revision))
        );
        assert!(
            act(
                FrontierAction::ContinueJourney,
                &mut ui,
                &mut s,
                &ResidentChunks::default(),
                &runtime(origin),
                &mut PlayerControl::default(),
                &mut queue
            )
            .is_none()
        );
        assert!(ui.panel == Panel::Fleet);
        assert_eq!(s.state, before);
    }

    /// Normal acceptance test and optional producer for a clean native review.
    /// The output folder is private, newly created and never a source fixture.
    #[test]
    fn prepared_company_review_journey_reaches_paid_setup_and_a_physical_loaded_hold() {
        use terra_bevy::rules::{CompanyCommand, FleetService, FleetStatus, TransportFacilityKind};
        use terra_bevy::sandbox::{LocalWorldSave, WorldIdentity};
        use terra_stream::{ChunkProvider, PreparedHeightmapProvider};

        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../terra-stream/data/heightmap-connected/manifest.json");
        let provider = PreparedHeightmapProvider::open(&source, 65).unwrap();
        let origin = GeoPosition::new(48.137, 11.575, 0.).unwrap();
        let key = TileKey::at(origin, 15).unwrap();
        let mut world_runtime = runtime(origin);
        world_runtime.config.heightmap_manifest = Some(source.canonicalize().unwrap());
        world_runtime.config.base_data_revision = provider.source_revision().to_owned();
        world_runtime.config.chunk_radius = 1;
        world_runtime.config.time_scale = 0.;
        world_runtime.config.live_weather = false;
        let resident = ResidentChunks {
            level: 15,
            chunks: key
                .neighbors(1)
                .into_iter()
                .map(|key| (key, Arc::new(provider.load(key).unwrap().chunk)))
                .collect(),
        };
        let mut time = Time::<Fixed>::from_seconds(0.05);
        time.advance_by(Duration::from_millis(50));
        let mut app = App::new();
        app.insert_resource(world_runtime)
            .insert_resource(resident)
            .insert_resource(SandboxRuntime::new(origin))
            .insert_resource(time)
            .init_resource::<terra_bevy::RegionalPersistence>()
            .init_resource::<terra_bevy::WorldDiagnostics>()
            .add_plugins(terra_bevy::TerraLivingPlugin);
        app.world_mut().run_schedule(Update);
        app.world_mut().run_schedule(FixedUpdate);
        let mut s = app.world_mut().remove_resource::<SandboxRuntime>().unwrap();
        assert_eq!(s.state.living.accounts[&s.player_id].money, 25_000);
        let garage = s
            .state
            .transport
            .facilities
            .values()
            .find(|f| f.kind == TransportFacilityKind::Garage)
            .unwrap()
            .position;
        let order = move_to(
            &mut s,
            garage,
            true,
            app.world().resource::<ResidentChunks>(),
            app.world().resource::<WorldRuntime>(),
            &mut PlayerControl::default(),
            &mut ControlQueue::default(),
        )
        .unwrap();
        s.state.apply(s.player_id, order, &s.catalog).unwrap();
        app.insert_resource(s);
        let mut approach_steps = 0;
        for _ in 0..4_000 {
            app.world_mut().run_schedule(FixedUpdate);
            approach_steps += 1;
            let s = app.world().resource::<SandboxRuntime>();
            if distance(s.state.players[&s.player_id].position, garage) <= 20. {
                break;
            }
        }
        let mut s = app.world_mut().remove_resource::<SandboxRuntime>().unwrap();
        assert!(
            distance(s.state.players[&s.player_id].position, garage) <= 20.,
            "physical garage approach failed"
        );
        company_command(
            &mut s,
            CompanyCommand::Create {
                name: "Valley Transit".into(),
            },
        );
        company_command(&mut s, CompanyCommand::Deposit { amount: 10_000 });
        let driver = company_command(&mut s, CompanyCommand::HireDriver { name: "Ari".into() })
            .created_id
            .unwrap();
        let vehicle = s
            .state
            .transport
            .vehicles
            .values()
            .find(|v| v.owner == s.player_id)
            .unwrap()
            .unit_id;
        let stops: Vec<_> = s
            .state
            .transport
            .facilities
            .values()
            .filter(|f| f.kind == TransportFacilityKind::Terminal)
            .take(2)
            .map(|f| f.id)
            .collect();
        let route = company_command(
            &mut s,
            CompanyCommand::CreateRoute {
                name: "Town service".into(),
                stops,
                service: FleetService::Passengers { count: 1 },
            },
        )
        .created_id
        .unwrap();
        company_command(
            &mut s,
            CompanyCommand::Assign {
                driver_id: driver,
                vehicle_id: vehicle,
                route_id: route,
            },
        );
        assert_eq!(s.state.living.accounts[&s.player_id].money, 13_000);
        assert_eq!(s.state.transport.companies[&s.player_id].cash, 9_000);
        let ready_state = s.state.clone();
        company_command(
            &mut s,
            CompanyCommand::SetDriverRunning {
                driver_id: driver,
                running: true,
            },
        );
        app.insert_resource(s);
        let mut service_steps = 0;
        for _ in 0..6_000 {
            app.world_mut().run_schedule(FixedUpdate);
            service_steps += 1;
            let s = app.world().resource::<SandboxRuntime>();
            if s.state.transport.companies[&s.player_id].drivers[0]
                .trip
                .is_some()
            {
                break;
            }
        }
        assert!(
            app.world()
                .resource::<terra_bevy::LivingDiagnostics>()
                .last_error
                .is_none()
        );
        let mut s = app.world_mut().remove_resource::<SandboxRuntime>().unwrap();
        assert!(
            s.state.transport.companies[&s.player_id].drivers[0]
                .trip
                .is_some(),
            "service did not physically load: {:?}",
            s.state.transport.companies[&s.player_id].drivers[0]
        );
        company_command(&mut s, CompanyCommand::HoldVehicle { driver_id: driver });
        assert_eq!(
            s.state.transport.companies[&s.player_id].drivers[0].status,
            FleetStatus::Held
        );
        let mut config = app.world().resource::<WorldRuntime>().config.clone();
        let mut clock = app.world().resource::<WorldRuntime>().clock;
        clock.paused = true;
        let saves = [("ready", ready_state), ("held", s.state.clone())].map(|(name, state)| {
            (
                name,
                LocalWorldSave {
                    schema_version: 1,
                    identity: WorldIdentity::from(&config),
                    clock,
                    player_id: s.player_id,
                    state,
                    politics: None,
                },
            )
        });
        for (_, save) in &saves {
            save.validate_for(&config, &s.catalog).unwrap();
        }
        if let Some(directory) = std::env::var_os("TERRA_COMPANY_REVIEW_OUTPUT") {
            let directory = std::path::PathBuf::from(directory);
            std::fs::create_dir(&directory).expect("choose a new review output folder");
            for (name, save) in saves {
                config.save_path = directory.join(format!("{name}.json"));
                config.cache_dir = directory.join("cache");
                save.write(&config.save_path).unwrap();
                let config_path = directory.join(format!("{name}-config.json"));
                std::fs::write(&config_path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
                let reread = LocalWorldSave::read(&config.save_path, &config, &s.catalog).unwrap();
                assert_eq!(reread.state, save.state);
            }
        }
        println!(
            "COMPANY_REVIEW_JOURNEY {}",
            serde_json::json!({
                "source_identity": config.base_data_revision,
                "starting_wallet": 25_000, "owner_cash_after_setup": 13_000,
                "company_cash_after_setup": 9_000,
                "approach_seconds": approach_steps as f64 * 0.05,
                "dispatch_to_load_seconds": service_steps as f64 * 0.05,
                "driver_id": driver, "vehicle_id": vehicle, "route_id": route,
                "loaded_held": true, "paid_commands_only": true, "positions_not_overwritten": true
            })
        );
    }

    /// Paid, physically approached rental for native presentation review. The
    /// optional private output is a review fixture, never a shipping save.
    #[test]
    fn prepared_comet_review_uses_paid_rental_and_physical_approach() {
        use terra_bevy::rules::{
            TransportCommand, TransportFacilityKind, VehicleModel, rental_offer,
        };
        use terra_bevy::sandbox::{LocalWorldSave, WorldIdentity};
        use terra_stream::{ChunkProvider, PreparedHeightmapProvider};

        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../terra-stream/data/heightmap-connected/manifest.json");
        let provider = PreparedHeightmapProvider::open(&source, 65).unwrap();
        let origin = GeoPosition::new(48.137, 11.575, 0.).unwrap();
        let key = TileKey::at(origin, 15).unwrap();
        let mut world_runtime = runtime(origin);
        world_runtime.config.heightmap_manifest = Some(source.canonicalize().unwrap());
        world_runtime.config.base_data_revision = provider.source_revision().to_owned();
        world_runtime.config.chunk_radius = 1;
        world_runtime.config.time_scale = 0.;
        world_runtime.config.live_weather = false;
        let resident = ResidentChunks {
            level: 15,
            chunks: key
                .neighbors(1)
                .into_iter()
                .map(|key| (key, Arc::new(provider.load(key).unwrap().chunk)))
                .collect(),
        };
        let mut time = Time::<Fixed>::from_seconds(0.05);
        time.advance_by(Duration::from_millis(50));
        let mut app = App::new();
        app.insert_resource(world_runtime)
            .insert_resource(resident)
            .insert_resource(SandboxRuntime::new(origin))
            .insert_resource(time)
            .init_resource::<terra_bevy::RegionalPersistence>()
            .init_resource::<terra_bevy::WorldDiagnostics>()
            .add_plugins(terra_bevy::TerraLivingPlugin);
        app.world_mut().run_schedule(Update);
        app.world_mut().run_schedule(FixedUpdate);
        let mut s = app.world_mut().remove_resource::<SandboxRuntime>().unwrap();
        let starting_money = s.state.living.accounts[&s.player_id].money;
        assert_eq!(starting_money, 25_000);
        let garage = s
            .state
            .transport
            .facilities
            .values()
            .find(|f| f.kind == TransportFacilityKind::Garage)
            .unwrap()
            .clone();
        let order = move_to(
            &mut s,
            garage.position,
            true,
            app.world().resource::<ResidentChunks>(),
            app.world().resource::<WorldRuntime>(),
            &mut PlayerControl::default(),
            &mut ControlQueue::default(),
        )
        .unwrap();
        s.state.apply(s.player_id, order, &s.catalog).unwrap();
        app.insert_resource(s);
        let mut approach_steps = 0;
        for _ in 0..4_000 {
            app.world_mut().run_schedule(FixedUpdate);
            approach_steps += 1;
            let s = app.world().resource::<SandboxRuntime>();
            if distance(s.state.players[&s.player_id].position, garage.position) <= 20. {
                break;
            }
        }
        let mut s = app.world_mut().remove_resource::<SandboxRuntime>().unwrap();
        assert!(distance(s.state.players[&s.player_id].position, garage.position) <= 20.);
        let offer = rental_offer(VehicleModel::Compact);
        let vehicle = s
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
            starting_money - offer.total
        );
        assert_eq!(
            s.state.transport.facilities[&garage.id].fuel_l,
            garage.fuel_l - 10.
        );
        assert_eq!(s.state.transport.vehicles[&vehicle].owner, s.player_id);
        assert_eq!(
            s.state.transport.vehicles[&vehicle]
                .rental
                .as_ref()
                .unwrap()
                .bond_escrow,
            offer.bond
        );
        let vehicle_position = s.state.living.units[&vehicle].position;
        let order = move_to(
            &mut s,
            vehicle_position,
            true,
            app.world().resource::<ResidentChunks>(),
            app.world().resource::<WorldRuntime>(),
            &mut PlayerControl::default(),
            &mut ControlQueue::default(),
        )
        .unwrap();
        s.state.apply(s.player_id, order, &s.catalog).unwrap();
        app.insert_resource(s);
        let mut vehicle_steps = 0;
        for _ in 0..4_000 {
            app.world_mut().run_schedule(FixedUpdate);
            vehicle_steps += 1;
            let s = app.world().resource::<SandboxRuntime>();
            if distance(s.state.players[&s.player_id].position, vehicle_position)
                <= terra_bevy::rules::INTERACTION_REACH_M
            {
                break;
            }
        }
        let mut s = app.world_mut().remove_resource::<SandboxRuntime>().unwrap();
        let boarding_distance = distance(s.state.players[&s.player_id].position, vehicle_position);
        assert!(boarding_distance <= terra_bevy::rules::INTERACTION_REACH_M);
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
        assert_eq!(s.state.living.accounts[&s.player_id].active_unit, vehicle);
        assert!(
            app.world()
                .resource::<terra_bevy::LivingDiagnostics>()
                .last_error
                .is_none()
        );
        let mut config = app.world().resource::<WorldRuntime>().config.clone();
        let mut clock = app.world().resource::<WorldRuntime>().clock;
        clock.paused = true;
        let save = LocalWorldSave {
            schema_version: 1,
            identity: WorldIdentity::from(&config),
            clock,
            player_id: s.player_id,
            state: s.state.clone(),
            politics: None,
        };
        save.validate_for(&config, &s.catalog).unwrap();
        if let Some(directory) = std::env::var_os("TERRA_COMET_REVIEW_OUTPUT") {
            let directory = std::path::PathBuf::from(directory);
            assert!(directory.is_absolute(), "review output must be absolute");
            std::fs::create_dir(&directory).expect("choose a new review output folder");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))
                    .unwrap();
            }
            config.save_path = directory.join("comet.json");
            config.cache_dir = directory.join("cache");
            save.write(&config.save_path).unwrap();
            std::fs::write(
                directory.join("comet-config.json"),
                serde_json::to_vec_pretty(&config).unwrap(),
            )
            .unwrap();
            let reread = LocalWorldSave::read(&config.save_path, &config, &s.catalog).unwrap();
            assert_eq!(reread.state, save.state);
        }
        println!(
            "COMET_REVIEW_JOURNEY {}",
            serde_json::json!({
                "source_identity": config.base_data_revision,
                "starting_wallet": starting_money,
                "rental_total": offer.total,
                "rental_bond": offer.bond,
                "remaining_wallet": s.state.living.accounts[&s.player_id].money,
                "garage_approach_seconds": approach_steps as f64 * 0.05,
                "vehicle_approach_seconds": vehicle_steps as f64 * 0.05,
                "boarding_distance_m": boarding_distance,
                "boarding_range_m": terra_bevy::rules::INTERACTION_REACH_M,
                "vehicle_id": vehicle,
                "paid_commands_only": true,
                "positions_not_overwritten": true,
                "active_rental": true,
                "review_starts_paused": true
            })
        );
    }

    #[test]
    fn guided_market_handoff_keeps_the_seller_through_the_displayed_trade() {
        let origin = GeoPosition::new(48.137, 11.575, 100.).unwrap();
        let mut sandbox = SandboxRuntime::new(origin);
        sandbox
            .state
            .seed_living_demo(origin, &[], &sandbox.catalog)
            .unwrap();
        sandbox
            .state
            .advance_settlements(0., &sandbox.catalog)
            .unwrap();
        let contract = *sandbox.state.settlements.contracts.keys().next().unwrap();
        let delivery = sandbox
            .state
            .settlements
            .contracts
            .get_mut(&contract)
            .unwrap();
        delivery.item = "planks".into();
        delivery.amount = 5;
        delivery.claimed_by = Some(1);
        sandbox.state.players.get_mut(&1).unwrap().inventory.clear();
        sandbox.state.resource_nodes.clear();
        let original = sandbox.state.living.shops.values().next().unwrap().clone();
        for shop in sandbox.state.living.shops.values_mut() {
            shop.stock.remove("planks");
        }
        sandbox
            .state
            .living
            .shops
            .get_mut(&original.id)
            .unwrap()
            .position = origin;
        let mut seller = original;
        seller.id = sandbox.state.living.next_id;
        sandbox.state.living.next_id += 1;
        seller.position = LocalFrame::new(origin).to_geo([8., 0., 0.]).unwrap();
        seller.stock.insert("planks".into(), 20);
        seller.prices.insert(
            "planks".into(),
            terra_bevy::rules::TradePrice { buy: 100, sell: 50 },
        );
        let chosen = seller.id;
        sandbox.state.living.shops.insert(chosen, seller);
        let mut ui = UiState::default();
        ui.frontier.guide_track = GuideTrack::Supply;
        ui.frontier.contract = Some(contract);
        cache_guidance(&sandbox, &mut ui);
        let before = sandbox.state.clone();
        let mut control = PlayerControl::default();
        let mut queue = ControlQueue::default();
        assert!(
            act(
                FrontierAction::ContinueJourney,
                &mut ui,
                &mut sandbox,
                &ResidentChunks::default(),
                &runtime(origin),
                &mut control,
                &mut queue,
            )
            .is_none()
        );
        assert_eq!(sandbox.state, before);
        assert!(queue.0.is_empty());
        assert!(ui.panel == Panel::Market);
        assert_eq!(ui.market.selected_shop, Some(chosen));
        crate::market::refresh_displayed(&sandbox, &mut ui);
        let item = crate::interface::TRADE_ITEMS
            .iter()
            .position(|(id, _)| *id == "planks")
            .unwrap();
        assert!(
            matches!(crate::market::trade_command(&sandbox, &ui, item, true).unwrap(),
            Command::Living { action: LivingCommand::TradeQuoted {shop_id, ..} } if shop_id == chosen)
        );
    }

    #[test]
    fn urgent_rest_opens_command_for_the_active_person_without_issuing_control() {
        let origin = GeoPosition::new(48.137, 11.575, 100.).unwrap();
        let mut sandbox = SandboxRuntime::new(origin);
        sandbox
            .state
            .seed_living_demo(origin, &[], &sandbox.catalog)
            .unwrap();
        let active = sandbox.state.living.accounts[&1].active_unit;
        sandbox
            .state
            .living
            .units
            .get_mut(&active)
            .unwrap()
            .needs
            .energy = 0.1;
        let mut ui = UiState::default();
        cache_guidance(&sandbox, &mut ui);
        let mut control = PlayerControl::default();
        control.selected.insert(999);
        let mut queue = ControlQueue::default();
        let before = sandbox.state.clone();
        let command = act(
            FrontierAction::ContinueJourney,
            &mut ui,
            &mut sandbox,
            &ResidentChunks::default(),
            &runtime(origin),
            &mut control,
            &mut queue,
        );
        assert!(command.is_none());
        assert!(queue.0.is_empty());
        assert_eq!(control.selected, std::collections::BTreeSet::from([active]));
        assert!(matches!(ui.panel, Panel::Command));
        assert!(ui.context_panel);
        assert_eq!(sandbox.state, before);
    }

    #[test]
    fn connected_valley_service_approach_reaches_a_funded_delivery_in_native_simulation() {
        use terra_stream::{ChunkProvider, PreparedHeightmapProvider};
        let provider = PreparedHeightmapProvider::open(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../terra-stream/data/heightmap-connected/manifest.json"),
            65,
        )
        .unwrap();
        let origin = GeoPosition::new(48.137, 11.575, 0.).unwrap();
        let key = TileKey::at(origin, 15).unwrap();
        let resident = ResidentChunks {
            level: 15,
            chunks: key
                .neighbors(1)
                .into_iter()
                .map(|key| (key, Arc::new(provider.load(key).unwrap().chunk)))
                .collect(),
        };
        let mut time = Time::<Fixed>::from_seconds(0.05);
        time.advance_by(Duration::from_millis(50));
        let mut app = App::new();
        app.insert_resource(runtime(origin))
            .insert_resource(resident)
            .insert_resource(SandboxRuntime::new(origin))
            .insert_resource(time)
            .init_resource::<terra_bevy::RegionalPersistence>()
            .init_resource::<terra_bevy::WorldDiagnostics>()
            .add_plugins(terra_bevy::TerraLivingPlugin);
        app.world_mut().run_schedule(Update);
        app.world_mut().run_schedule(FixedUpdate);
        let mut sandbox = app.world_mut().remove_resource::<SandboxRuntime>().unwrap();
        let contract = sandbox
            .state
            .settlements
            .contracts
            .values()
            .find(|contract| {
                sandbox
                    .state
                    .living
                    .producers
                    .get(&contract.target_site)
                    .is_some_and(|site| site.kind == terra_bevy::rules::ProductionKind::Sawmill)
                    && sandbox.state.players[&1]
                        .inventory
                        .get(&contract.item)
                        .copied()
                        .unwrap_or(0)
                        >= contract.amount
            })
            .expect("the authored sawmill posts a funded starter supply request")
            .clone();
        sandbox
            .state
            .apply(
                1,
                Command::Settlement {
                    action: SettlementCommand::AcceptContract {
                        contract_id: contract.id,
                    },
                },
                &sandbox.catalog,
            )
            .unwrap();
        let mut control = PlayerControl::default();
        let mut queue = ControlQueue::default();
        let command = move_to(
            &mut sandbox,
            contract.target_position,
            true,
            app.world().resource::<ResidentChunks>(),
            app.world().resource::<WorldRuntime>(),
            &mut control,
            &mut queue,
        )
        .expect("the loaded sawmill has a supported approach");
        assert!(matches!(queue.0.pop_front(), Some(ControlIntent::Release)));
        sandbox.state.apply(1, command, &sandbox.catalog).unwrap();
        app.insert_resource(sandbox);
        // Exercise the same collision, terrain and actor systems as the native
        // game rather than placing the player directly at the requested goal.
        for _ in 0..3000 {
            app.world_mut().run_schedule(FixedUpdate);
            let sandbox = app.world().resource::<SandboxRuntime>();
            if distance(
                sandbox.state.living.active_unit(1).unwrap().position,
                contract.target_position,
            ) <= terra_bevy::rules::INTERACTION_REACH_M
            {
                break;
            }
        }
        assert!(
            app.world()
                .resource::<terra_bevy::LivingDiagnostics>()
                .last_error
                .is_none()
        );
        let mut sandbox = app.world_mut().remove_resource::<SandboxRuntime>().unwrap();
        let position = sandbox.state.living.active_unit(1).unwrap().position;
        assert!(
            distance(position, contract.target_position) <= terra_bevy::rules::INTERACTION_REACH_M,
            "service approach stopped {:.2} metres away",
            distance(position, contract.target_position)
        );
        assert!(
            sandbox
                .state
                .living
                .facility_footprints()
                .iter()
                .all(|(center, radius)| center.surface_distance_m(position) > radius + 0.35)
        );
        let before = sandbox.state.living.accounts[&1].money;
        sandbox
            .state
            .apply(
                1,
                Command::Settlement {
                    action: SettlementCommand::FulfillContract {
                        contract_id: contract.id,
                    },
                },
                &sandbox.catalog,
            )
            .unwrap();
        assert_eq!(
            sandbox.state.living.accounts[&1].money,
            before + contract.reward
        );
        assert!(
            !sandbox
                .state
                .settlements
                .contracts
                .contains_key(&contract.id)
        );
    }

    #[test]
    fn vehicle_service_order_requires_dry_wheels_and_body_not_only_a_dry_center() {
        let key = TileKey::at(GeoPosition::new(48.137, 11.575, 100.).unwrap(), 15).unwrap();
        let mut target = key.center();
        target.altitude_m = 100.;
        let frame = LocalFrame::new(target);
        let seed = frame.to_geo([0., 0., 200.]).unwrap();
        let mut sandbox = SandboxRuntime::new(seed);
        sandbox
            .state
            .seed_living_demo(seed, &[], &sandbox.catalog)
            .unwrap();
        sandbox
            .state
            .ensure_transport_demo(&sandbox.catalog)
            .unwrap();
        sandbox.state.living.roads.clear();
        let unit = sandbox
            .state
            .transport
            .vehicles
            .values()
            .find(|v| v.owner == 1)
            .unwrap()
            .unit_id;
        sandbox.state.living.units.get_mut(&unit).unwrap().position =
            frame.to_geo([-30., 0., 0.]).unwrap();
        sandbox
            .state
            .apply(
                1,
                Command::Living {
                    action: LivingCommand::SelectActive { unit_id: unit },
                },
                &sandbox.catalog,
            )
            .unwrap();
        // A 40 cm strip is dry at the approach centers, but cannot support a
        // road vehicle's wheels or body. The rest is explicitly mapped water.
        let features = [(-100., -0.2), (0.2, 100.)]
            .into_iter()
            .enumerate()
            .map(|(i, (near, far))| WorldFeature {
                id: format!("water:{i}"),
                kind: FeatureKind::Water,
                points: [[-100., near], [100., near], [100., far], [-100., far]]
                    .into_iter()
                    .map(|[x, z]| frame.to_geo([x, 0., z]).unwrap())
                    .collect(),
                height_m: 0.,
                properties: BTreeMap::new(),
            })
            .collect();
        let mut resident = ResidentChunks {
            level: 15,
            chunks: BTreeMap::from([(
                key,
                Arc::new(WorldChunk {
                    key,
                    heights: HeightField::constant(100.),
                    features,
                    sources: vec![],
                    warnings: vec![],
                }),
            )]),
        };
        let runtime = runtime(target);
        let dry_center = frame.to_geo([-8., 0., 0.]).unwrap();
        assert!(
            terra_bevy::sandbox::placement_error(
                &Command::ClaimPlot {
                    center: dry_center,
                    radius_m: 5.
                },
                &sandbox,
                &runtime,
                &resident
            )
            .is_none()
        );
        let mut control = PlayerControl::default();
        let mut queue = ControlQueue::default();
        assert!(
            move_to(
                &mut sandbox,
                target,
                true,
                &resident,
                &runtime,
                &mut control,
                &mut queue
            )
            .is_none()
        );
        assert!(queue.0.is_empty());
        assert!(control.last_order.is_none());
        assert!(control.selected.is_empty());
        assert!(
            sandbox
                .last_message
                .contains("No supported, unoccupied approach")
        );
        // With the same actors, target and ownership on continuous dry ground,
        // a move is available and keeps the interaction endpoint in reach.
        Arc::make_mut(resident.chunks.get_mut(&key).unwrap())
            .features
            .clear();
        let command = move_to(
            &mut sandbox,
            target,
            true,
            &resident,
            &runtime,
            &mut control,
            &mut queue,
        )
        .unwrap();
        let Command::Living {
            action:
                LivingCommand::MoveUnits {
                    unit_ids,
                    destination,
                },
        } = command
        else {
            panic!("expected move command")
        };
        assert_eq!(unit_ids, vec![unit]);
        assert!(distance(destination, target) <= terra_bevy::rules::INTERACTION_REACH_M);
    }

    #[test]
    fn final_city_tool_contribution_submits_only_the_remaining_three() {
        let origin = GeoPosition::new(48.137, 11.575, 100.).unwrap();
        let mut sandbox = SandboxRuntime::new(origin);
        sandbox
            .state
            .seed_living_demo(origin, &[], &sandbox.catalog)
            .unwrap();
        sandbox
            .state
            .apply(
                1,
                Command::Settlement {
                    action: SettlementCommand::Found {
                        name: "Public works fixture".into(),
                        center: origin,
                        faction_owner: None,
                    },
                },
                &sandbox.catalog,
            )
            .unwrap();
        let id = *sandbox.state.settlements.territories.keys().next().unwrap();
        let territory = sandbox.state.settlements.territories.get_mut(&id).unwrap();
        territory.tier = terra_bevy::rules::SettlementTier::Village;
        territory.stock.insert("tool".into(), 5);
        sandbox
            .state
            .players
            .get_mut(&1)
            .unwrap()
            .inventory
            .insert("tool".into(), 6);
        let mut ui = UiState::default();
        ui.frontier.contribution = 2;
        let command = contribute(&sandbox, &ui).unwrap();
        sandbox.state.apply(1, command, &sandbox.catalog).unwrap();
        assert_eq!(sandbox.state.settlements.territories[&id].stock["tool"], 8);
        assert_eq!(sandbox.state.players[&1].inventory["tool"], 3);
        assert!(unavailable(
            FrontierAction::Contribute,
            &sandbox,
            &ui,
            false
        ));
    }

    #[test]
    fn partial_replica_does_not_block_host_building_requirements_but_keeps_known_costs() {
        let origin = GeoPosition::new(48.137, 11.575, 100.).unwrap();
        let mut sandbox = SandboxRuntime::new(origin);
        sandbox
            .state
            .seed_living_demo(origin, &[], &sandbox.catalog)
            .unwrap();
        sandbox
            .state
            .apply(
                1,
                Command::Settlement {
                    action: SettlementCommand::Found {
                        name: "Partial-view charter".into(),
                        center: origin,
                        faction_owner: None,
                    },
                },
                &sandbox.catalog,
            )
            .unwrap();
        let id = *sandbox.state.settlements.territories.keys().next().unwrap();
        let territory = sandbox.state.settlements.territories.get_mut(&id).unwrap();
        territory.stock.insert("planks".into(), 30);
        territory.stock.insert("stone_blocks".into(), 10);
        sandbox
            .state
            .settlements
            .progress
            .get_mut(&1)
            .unwrap()
            .experience = 50;
        let ui = UiState::default();
        // Two houses may have been omitted behind earlier structure IDs by the
        // bounded network view. Authority still checks them when intent arrives.
        assert!(unavailable(FrontierAction::Upgrade, &sandbox, &ui, false));
        assert!(!unavailable(FrontierAction::Upgrade, &sandbox, &ui, true));
        sandbox
            .state
            .settlements
            .territories
            .get_mut(&id)
            .unwrap()
            .stock
            .clear();
        assert!(unavailable(FrontierAction::Upgrade, &sandbox, &ui, true));
    }
}
