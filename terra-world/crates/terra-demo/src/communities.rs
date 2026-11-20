//! Native community lifecycle. Reviews retain the exact faction, player and
//! property selected; the authoritative domain validates each resulting intent.
use crate::interface::{self, Action, InputMode, Label, Panel, UiState};
use bevy::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
use terra_bevy::rules::Command;
use terra_bevy::{SandboxRuntime, rules::*};

const ROSTER_PAGE_SIZE: usize = 8;
type PropertyIdentity = (u64, u64, Option<u64>);

#[derive(Default)]
pub struct CommunityUi {
    pub invitation: Option<u64>,
    displayed_invitation: Option<DisplayedInvitation>,
    roster_page: usize,
    input: Option<CommunityContext>,
    review: Option<CommunityReview>,
    displayed_review: Option<u64>,
    /// Reveal a newly prepared review in the inspector's existing scroll area.
    pub scroll_revision: u64,
}

#[derive(Clone, Copy)]
pub enum CommunityAction {
    NextInvitation,
    DeclineInvitation,
    RevokeInvitation,
    NextRosterPage,
    RemoveMember,
    TransferLeadership,
    Leave,
    Dissolve,
    Confirm,
    ConfirmReviewed(u64),
    Cancel,
}

#[derive(Clone, Copy)]
pub enum CommunityLabel {
    Invitation,
    Roster,
    Review,
}

#[derive(Clone)]
struct CommunityContext {
    player_id: u64,
    restore_revision: u64,
    faction: Faction,
}

#[derive(Clone, PartialEq, Eq)]
struct DisplayedInvitation {
    player_id: u64,
    restore_revision: u64,
    faction: Faction,
}

#[derive(Clone)]
struct CommunityReview {
    id: u64,
    context: CommunityContext,
    change: ReviewedChange,
}

#[derive(Clone)]
enum ReviewedChange {
    Leave,
    Dissolve,
    Remove(u64),
    Transfer(u64),
    Share {
        plot: LandPlot,
        structures: Vec<PropertyIdentity>,
        farms: Vec<PropertyIdentity>,
    },
}

pub fn selected<'a>(s: &'a SandboxRuntime, ui: &UiState) -> Option<&'a Faction> {
    match ui.selected_faction {
        Some(id) => s
            .state
            .factions
            .get(&id)
            .filter(|f| f.members.contains_key(&s.player_id)),
        None => s
            .state
            .factions
            .values()
            .find(|f| f.members.contains_key(&s.player_id)),
    }
}

fn invitation<'a>(s: &'a SandboxRuntime, ui: &UiState) -> Option<&'a Faction> {
    ui.communities
        .invitation
        .and_then(|id| s.state.factions.get(&id))
        .filter(|f| f.invites.contains(&s.player_id))
        .or_else(|| {
            s.state
                .factions
                .values()
                .find(|f| f.invites.contains(&s.player_id))
        })
}

fn current_invitation(s: &SandboxRuntime, ui: &UiState) -> Option<DisplayedInvitation> {
    Some(DisplayedInvitation {
        player_id: s.player_id,
        restore_revision: s.restore_revision,
        faction: invitation(s, ui)?.clone(),
    })
}

/// Label refresh and retained identity are one operation; a disappearing invite
/// cannot make an Accept/Decline click target a different community.
pub fn refresh_displayed(s: &SandboxRuntime, ui: &mut UiState) {
    ui.communities.displayed_invitation = current_invitation(s, ui);
    ui.communities.displayed_review = ui
        .communities
        .review
        .as_ref()
        .filter(|review| {
            ui.context_panel && ui.panel == Panel::Factions && review.is_current(s, ui)
        })
        .map(|review| review.id);
}

/// Bind activation to the review that was actually displayed, before the
/// native action queue can process cancel, selection or replacement reviews.
pub fn bind_review_action(ui: &UiState, action: CommunityAction) -> Option<CommunityAction> {
    match action {
        CommunityAction::Confirm => ui
            .communities
            .displayed_review
            .map(CommunityAction::ConfirmReviewed),
        action => Some(action),
    }
}

fn invitation_command(s: &mut SandboxRuntime, ui: &UiState, accept: bool) -> Option<Command> {
    let shown = ui.communities.displayed_invitation.as_ref();
    if shown.is_none() || shown != current_invitation(s, ui).as_ref() {
        s.last_message = "The shown invitation changed. Review the named invitation before accepting or declining.".into();
        return None;
    }
    let faction_id = shown.expect("displayed invitation checked").faction.id;
    Some(if accept {
        Command::AcceptFactionInvite { faction_id }
    } else {
        Command::DeclineFactionInvite { faction_id }
    })
}

impl CommunityContext {
    fn capture(s: &SandboxRuntime, ui: &UiState) -> Option<Self> {
        Some(Self {
            player_id: s.player_id,
            restore_revision: s.restore_revision,
            faction: selected(s, ui)?.clone(),
        })
    }

    fn is_current(&self, s: &SandboxRuntime, ui: &UiState) -> bool {
        self.player_id == s.player_id
            && self.restore_revision == s.restore_revision
            && selected(s, ui) == Some(&self.faction)
    }
}

fn property(s: &SandboxRuntime, plot: &LandPlot) -> (Vec<PropertyIdentity>, Vec<PropertyIdentity>) {
    let structures = s
        .state
        .structures
        .values()
        .filter(|body| {
            plot.center.surface_distance_m(body.position) + body.radius_m <= plot.radius_m
        })
        .map(|body| (body.id, body.owner, body.faction_owner))
        .collect();
    let farms = s
        .state
        .farms
        .values()
        .filter(|farm| farm.plot_id == plot.id)
        .map(|farm| (farm.id, farm.owner, farm.faction_owner))
        .collect();
    (structures, farms)
}

impl CommunityReview {
    fn is_current(&self, s: &SandboxRuntime, ui: &UiState) -> bool {
        self.context.is_current(s, ui)
            && match &self.change {
                ReviewedChange::Dissolve => s
                    .state
                    .faction_dissolution_readiness(s.player_id, self.context.faction.id)
                    .is_ok(),
                ReviewedChange::Share {
                    plot,
                    structures,
                    farms,
                } => {
                    s.state.plots.get(&plot.id) == Some(plot)
                        && property(s, plot) == (structures.clone(), farms.clone())
                }
                _ => true,
            }
    }

    fn describe(&self) -> String {
        let faction = &self.context.faction;
        let name = format!("{} #{}", faction.name, faction.id);
        let change = match &self.change {
            ReviewedChange::Dissolve => {
                let invitation_word = if faction.invites.len() == 1 {
                    "invitation"
                } else {
                    "invitations"
                };
                return format!(
                    "Dissolve {name}?\nYou are its only member. Revokes {} {invitation_word} and frees those slots.\nRequires no shared land or settlement. The authority rechecks; nearby data cannot prove this. No property, public stock or funds move; no refund.\nCancel keeps the faction. Dissolution is permanent; a new faction has a new ID.",
                    faction.invites.len()
                );
            }
            ReviewedChange::Leave => format!(
                "Leave {name}? Shared access ends and any home in this faction is unset. Rejoining requires a new invitation."
            ),
            ReviewedChange::Remove(player) => format!(
                "Remove player #{player} from {name}? Their shared access ends and any home in this faction is unset, including while offline."
            ),
            ReviewedChange::Transfer(player) => format!(
                "Make player #{player} leader of {name}? You become a Builder. Only the new leader can manage people or give leadership back."
            ),
            ReviewedChange::Share {
                plot,
                structures,
                farms,
            } => format!(
                "Give plot #{} to {name}? Its {} loaded structures and {} loaded crops follow faction roles. This ownership transfer has no undo or personal reclaim action.",
                plot.id,
                structures.len(),
                farms.len()
            ),
        };
        format!(
            "Review change\n{change}\nShared assets, public stock and treasury remain with the faction. Contributions are not refunded."
        )
    }

    fn command(self) -> Command {
        let faction_id = self.context.faction.id;
        match self.change {
            ReviewedChange::Leave => Command::LeaveFaction { faction_id },
            ReviewedChange::Dissolve => Command::DissolveFaction { faction_id },
            ReviewedChange::Remove(player_id) => Command::RemoveFactionMember {
                faction_id,
                player_id,
            },
            ReviewedChange::Transfer(player_id) => Command::TransferFactionLeadership {
                faction_id,
                player_id,
            },
            ReviewedChange::Share { plot, .. } => Command::TransferPlotToFaction {
                faction_id,
                plot_id: plot.id,
            },
        }
    }
}

fn prepare(change: ReviewedChange, ui: &mut UiState, s: &mut SandboxRuntime) {
    let Some(context) = CommunityContext::capture(s, ui) else {
        s.last_message = "Choose a faction you belong to before changing community access.".into();
        return;
    };
    // Process-wide uniqueness survives a UiState reset without serializing any
    // review token into a save. Exhaustion refuses instead of reusing consent.
    static NEXT_REVIEW_ID: AtomicU64 = AtomicU64::new(1);
    let mut candidate = NEXT_REVIEW_ID.load(Ordering::Relaxed);
    let id = loop {
        let Some(next) = candidate.checked_add(1) else {
            s.last_message =
                "Community review IDs are exhausted. Restart before reviewing another change."
                    .into();
            return;
        };
        match NEXT_REVIEW_ID.compare_exchange_weak(
            candidate,
            next,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => break candidate,
            Err(current) => candidate = current,
        }
    };
    ui.communities.review = Some(CommunityReview {
        id,
        context,
        change,
    });
    ui.communities.displayed_review = None;
    ui.communities.scroll_revision = ui.communities.scroll_revision.wrapping_add(1);
    ui.context_panel = true;
    ui.panel = Panel::Factions;
    ui.panel_navigation_open = false;
    s.last_message =
        "Review the named community change in Factions, then confirm or cancel.".into();
}

pub fn start_input(mode: InputMode, ui: &mut UiState, s: &mut SandboxRuntime) {
    let context = CommunityContext::capture(s, ui);
    if mode != InputMode::FactionName
        && context
            .as_ref()
            .is_none_or(|c| c.faction.leader != s.player_id)
    {
        s.last_message = "Only the selected faction's current leader can manage its people.".into();
        return;
    }
    ui.communities.review = None;
    ui.communities.displayed_review = None;
    ui.communities.input = context;
    ui.input_mode = mode;
    ui.gps_text.clear();
    ui.editing_gps = true;
}

pub fn finish_input(
    mode: InputMode,
    value: &str,
    ui: &mut UiState,
    s: &mut SandboxRuntime,
) -> Option<Command> {
    let Some(context) = ui.communities.input.take().filter(|c| c.is_current(s, ui)) else {
        s.last_message =
            "The player, faction or roster changed. Start the community action again.".into();
        return None;
    };
    let Some(player_id) = value.parse::<u64>().ok().filter(|id| *id != 0) else {
        s.last_message =
            "Enter a positive player ID from the community roster or a registered player.".into();
        return None;
    };
    let faction_id = context.faction.id;
    match mode {
        InputMode::InvitePlayer => Some(Command::InviteToFaction {
            faction_id,
            player_id,
        }),
        InputMode::PromotePlayer | InputMode::DemotePlayer => Some(Command::SetFactionRole {
            faction_id,
            player_id,
            role: if mode == InputMode::PromotePlayer {
                FactionRole::Builder
            } else {
                FactionRole::Member
            },
        }),
        InputMode::RevokeInvite => Some(Command::RevokeFactionInvite {
            faction_id,
            player_id,
        }),
        InputMode::RemoveMember | InputMode::TransferLeader => {
            if player_id == context.faction.leader
                || !context.faction.members.contains_key(&player_id)
            {
                s.last_message =
                    "Choose another existing member's player ID from the roster.".into();
                return None;
            }
            prepare(
                if mode == InputMode::RemoveMember {
                    ReviewedChange::Remove(player_id)
                } else {
                    ReviewedChange::Transfer(player_id)
                },
                ui,
                s,
            );
            None
        }
        _ => None,
    }
}

pub fn accept_invitation(s: &mut SandboxRuntime, ui: &UiState) -> Option<Command> {
    invitation_command(s, ui, true)
}

pub fn review_share_plot(ui: &mut UiState, s: &mut SandboxRuntime) {
    let Some(position) = s.state.players.get(&s.player_id).map(|p| p.position) else {
        return;
    };
    let Some(faction) = selected(s, ui) else {
        s.last_message =
            "Select a faction you lead and stand within 20 m of your personal plot.".into();
        return;
    };
    if faction.leader != s.player_id {
        s.last_message =
            "Only the current faction leader can transfer personal land into it.".into();
        return;
    }
    let plot = s
        .state
        .plots
        .values()
        .filter(|p| {
            p.owner == s.player_id
                && p.faction_owner.is_none()
                && position
                    .surface_distance_m(p.center)
                    .hypot(position.altitude_m - p.center.altitude_m)
                    <= INTERACTION_REACH_M
        })
        .min_by(|a, b| {
            a.center
                .surface_distance_m(position)
                .total_cmp(&b.center.surface_distance_m(position))
        })
        .cloned();
    let Some(plot) = plot else {
        s.last_message =
            "Stand within 20 m of the center of a personal plot before sharing it.".into();
        return;
    };
    let (structures, farms) = property(s, &plot);
    prepare(
        ReviewedChange::Share {
            plot,
            structures,
            farms,
        },
        ui,
        s,
    );
}

pub fn act(action: CommunityAction, ui: &mut UiState, s: &mut SandboxRuntime) -> Option<Command> {
    match action {
        CommunityAction::NextInvitation => {
            let ids: Vec<_> = s
                .state
                .factions
                .values()
                .filter(|f| f.invites.contains(&s.player_id))
                .map(|f| f.id)
                .collect();
            let current = invitation(s, ui).map(|f| f.id);
            let next = ids
                .iter()
                .position(|id| Some(*id) == current)
                .map_or(0, |i| (i + 1) % ids.len().max(1));
            ui.communities.invitation = ids.get(next).copied();
            ui.communities.displayed_invitation = None;
        }
        CommunityAction::DeclineInvitation => return invitation_command(s, ui, false),
        CommunityAction::RevokeInvitation => start_input(InputMode::RevokeInvite, ui, s),
        CommunityAction::RemoveMember => start_input(InputMode::RemoveMember, ui, s),
        CommunityAction::TransferLeadership => start_input(InputMode::TransferLeader, ui, s),
        CommunityAction::Leave => {
            if selected(s, ui).is_some_and(|f| f.leader == s.player_id) {
                s.last_message = "Hand over leadership before leaving, or review dissolution if you are the sole member and the faction has no shared land or settlement.".into();
            } else {
                prepare(ReviewedChange::Leave, ui, s);
            }
        }
        CommunityAction::Dissolve => {
            let Some(faction) = selected(s, ui) else {
                s.last_message = "Choose the faction you lead before reviewing dissolution.".into();
                return None;
            };
            if let Err(error) = s
                .state
                .faction_dissolution_readiness(s.player_id, faction.id)
            {
                s.last_message = format!("This faction cannot dissolve: {error}");
            } else {
                prepare(ReviewedChange::Dissolve, ui, s);
            }
        }
        CommunityAction::NextRosterPage => {
            if let Some(faction) = selected(s, ui) {
                let pages = (faction.members.len() + faction.invites.len())
                    .div_ceil(ROSTER_PAGE_SIZE)
                    .max(1);
                ui.communities.roster_page = (ui.communities.roster_page + 1) % pages;
            }
        }
        CommunityAction::Confirm => {
            s.last_message = "Review the displayed community change before confirming.".into();
        }
        CommunityAction::ConfirmReviewed(id) => {
            if ui
                .communities
                .review
                .as_ref()
                .is_none_or(|review| review.id != id)
            {
                s.last_message = "That confirmation belongs to an earlier review. Review the current change before confirming.".into();
                return None;
            }
            let Some(review) = ui.communities.review.take() else {
                return None;
            };
            ui.communities.displayed_review = None;
            if review.is_current(s, ui) {
                return Some(review.command());
            }
            s.last_message = "This review expired because the player, faction, roster or property changed. Start the action again.".into();
        }
        CommunityAction::Cancel => {
            ui.communities.review = None;
            ui.communities.displayed_review = None;
            s.last_message = "Community change canceled. No command was sent.".into();
        }
    }
    None
}

pub fn faction_changed(ui: &mut UiState) {
    ui.communities.review = None;
    ui.communities.displayed_review = None;
    ui.communities.input = None;
    ui.communities.roster_page = 0;
}

pub fn has_review(ui: &UiState) -> bool {
    ui.communities.review.is_some()
}

pub fn unavailable(action: CommunityAction, s: &SandboxRuntime, ui: &UiState) -> bool {
    let faction = selected(s, ui);
    match action {
        CommunityAction::NextInvitation => {
            s.state
                .factions
                .values()
                .filter(|f| f.invites.contains(&s.player_id))
                .count()
                < 2
        }
        CommunityAction::DeclineInvitation => invitation(s, ui).is_none(),
        CommunityAction::RevokeInvitation => {
            faction.is_none_or(|f| f.leader != s.player_id || f.invites.is_empty())
        }
        CommunityAction::RemoveMember | CommunityAction::TransferLeadership => {
            faction.is_none_or(|f| f.leader != s.player_id || f.members.len() < 2)
        }
        CommunityAction::Leave => faction.is_none_or(|f| f.leader == s.player_id),
        CommunityAction::Dissolve => faction.is_none_or(|f| {
            s.state
                .faction_dissolution_readiness(s.player_id, f.id)
                .is_err()
        }),
        CommunityAction::NextRosterPage => {
            faction.is_none_or(|f| f.members.len() + f.invites.len() <= ROSTER_PAGE_SIZE)
        }
        CommunityAction::Confirm => ui
            .communities
            .review
            .as_ref()
            .is_none_or(|r| !r.is_current(s, ui)),
        CommunityAction::ConfirmReviewed(id) => ui
            .communities
            .review
            .as_ref()
            .is_none_or(|review| review.id != id || !review.is_current(s, ui)),
        CommunityAction::Cancel => ui.communities.review.is_none(),
    }
}

pub fn legacy_unavailable(action: Action, s: &SandboxRuntime, ui: &UiState) -> bool {
    match action {
        Action::CreateFaction => {
            s.state
                .factions
                .values()
                .filter(|f| {
                    f.members.contains_key(&s.player_id) || f.invites.contains(&s.player_id)
                })
                .count()
                >= MAX_PLAYER_FACTIONS
        }
        Action::NextFaction => {
            s.state
                .factions
                .values()
                .filter(|f| f.members.contains_key(&s.player_id))
                .count()
                < if ui.selected_faction.is_some() && selected(s, ui).is_none() {
                    1
                } else {
                    2
                }
        }
        Action::AcceptFaction => invitation(s, ui).is_none(),
        Action::InviteFaction
        | Action::PromoteFaction
        | Action::DemoteFaction
        | Action::SharePlot => selected(s, ui).is_none_or(|f| f.leader != s.player_id),
        _ => false,
    }
}

pub fn summary(s: &SandboxRuntime, ui: &UiState) -> String {
    let Some(faction) = selected(s, ui) else {
        if let Some(id) = ui.selected_faction {
            return format!(
                "Selected faction #{id} is unavailable or you are no longer a member. Choose Select next faction to review an available membership."
            );
        }
        return format!(
            "You: player #{}\nCreate a faction or accept a named invitation to share homes, land and public works.",
            s.player_id
        );
    };
    let role = faction.members[&s.player_id];
    let member_word = if faction.members.len() == 1 {
        "member"
    } else {
        "members"
    };
    let invitation_word = if faction.invites.len() == 1 {
        "invitation"
    } else {
        "invitations"
    };
    let permissions = match role {
        FactionRole::Leader => {
            "You manage people and public works. Hand over leadership before leaving. A sole-member faction can dissolve only while it has no shared land or settlement."
        }
        FactionRole::Builder => {
            "You can build and manage public works. The leader manages membership."
        }
        FactionRole::Member => {
            "You can use shared housing and workbenches. Ask the leader for building rights."
        }
    };
    format!(
        "You: player #{}\n{} · #{}\nRole: {role:?} · Leader: #{}\n{} {member_word} · {} {invitation_word}\n{permissions}",
        s.player_id,
        faction.name,
        faction.id,
        faction.leader,
        faction.members.len(),
        faction.invites.len()
    )
}

pub fn describe(label: CommunityLabel, s: &SandboxRuntime, ui: &UiState) -> String {
    match label {
        CommunityLabel::Invitation => invitation(s, ui).map(|f| format!("Invitation: {} · #{}\nLeader #{} invites you as a Member.", f.name, f.id, f.leader))
            .unwrap_or_else(|| "No pending invitations. Give your player ID to a faction leader to join.".into()),
        CommunityLabel::Roster => {
            let Some(faction) = selected(s, ui) else { return "Your community's people and roles appear here after you join.".into(); };
            let entries: Vec<_> = faction.members.iter().map(|(id, role)| format!("Player #{id} · {role:?}"))
                .chain(faction.invites.iter().map(|id| format!("Player #{id} · invited"))).collect();
            let pages = entries.len().div_ceil(ROSTER_PAGE_SIZE).max(1);
            let page = ui.communities.roster_page.min(pages - 1);
            format!("People · page {} of {pages}\n{}", page + 1, entries.iter().skip(page * ROSTER_PAGE_SIZE).take(ROSTER_PAGE_SIZE).cloned().collect::<Vec<_>>().join("\n"))
        }
        CommunityLabel::Review => ui.communities.review.as_ref().map(|review| {
            if review.is_current(s, ui) { review.describe() }
            else { "Review expired. The player, community, roster or property changed. Cancel and start the action again.".into() }
        }).unwrap_or_default(),
    }
}

pub fn build(parent: &mut ChildSpawnerCommands) {
    parent.spawn((
        Label::Community(CommunityLabel::Review),
        interface::text("", 12., interface::SAND),
    ));
    interface::button(
        parent,
        "Cancel community change",
        Action::Community(CommunityAction::Cancel),
    );
    interface::button(
        parent,
        "Confirm community change",
        Action::Community(CommunityAction::Confirm),
    );
    parent.spawn((Label::Faction, interface::text("", 12., interface::TEXT)));
    interface::button(parent, "Select next faction", Action::NextFaction);
    interface::button(parent, "Create a named faction", Action::CreateFaction);
    interface::heading(parent, "Invitations to you");
    parent.spawn((
        Label::Community(CommunityLabel::Invitation),
        interface::text("", 12., interface::TEXT),
    ));
    interface::button(
        parent,
        "Choose next invitation",
        Action::Community(CommunityAction::NextInvitation),
    );
    interface::button(parent, "Accept shown invitation", Action::AcceptFaction);
    interface::button(
        parent,
        "Decline shown invitation",
        Action::Community(CommunityAction::DeclineInvitation),
    );
    interface::heading(parent, "People and access");
    parent.spawn((
        Label::Community(CommunityLabel::Roster),
        interface::text("", 12., interface::TEXT),
    ));
    interface::button(
        parent,
        "Next roster page",
        Action::Community(CommunityAction::NextRosterPage),
    );
    interface::button(parent, "Invite a player by ID", Action::InviteFaction);
    interface::button(
        parent,
        "Revoke an invitation by ID",
        Action::Community(CommunityAction::RevokeInvitation),
    );
    interface::button(parent, "Grant builder role", Action::PromoteFaction);
    interface::button(parent, "Set member role", Action::DemoteFaction);
    interface::button(
        parent,
        "Review removing a member",
        Action::Community(CommunityAction::RemoveMember),
    );
    interface::button(
        parent,
        "Review leadership handover",
        Action::Community(CommunityAction::TransferLeadership),
    );
    interface::button(
        parent,
        "Review leaving this faction",
        Action::Community(CommunityAction::Leave),
    );
    interface::button(
        parent,
        "Review dissolving unused faction",
        Action::Community(CommunityAction::Dissolve),
    );
    interface::heading(parent, "Shared property");
    parent.spawn(interface::text("The leader may give a personal plot to the faction from within 20 m of its center. Ownership transfer is permanent. Leaving or losing a role does not return property, public stock or treasury contributions.", 12., interface::MUTED));
    interface::button(parent, "Review sharing nearest plot", Action::SharePlot);
}

#[cfg(test)]
mod tests {
    use super::*;
    use terra_core::GeoPosition;

    fn fixture() -> (SandboxRuntime, UiState, u64) {
        let origin = GeoPosition::new(48.137, 11.575, 100.).unwrap();
        let mut s = SandboxRuntime::new(origin);
        for player in [2, 3, 4] {
            s.state.add_player(player, origin).unwrap();
        }
        let faction = apply(
            &mut s,
            1,
            Command::CreateFaction {
                name: "River Commons".into(),
            },
        );
        for player in [2, 3] {
            apply(
                &mut s,
                1,
                Command::InviteToFaction {
                    faction_id: faction,
                    player_id: player,
                },
            );
            apply(
                &mut s,
                player,
                Command::AcceptFactionInvite {
                    faction_id: faction,
                },
            );
        }
        let ui = UiState {
            selected_faction: Some(faction),
            panel: Panel::Factions,
            ..Default::default()
        };
        (s, ui, faction)
    }

    fn apply(s: &mut SandboxRuntime, player: u64, command: Command) -> u64 {
        s.state
            .apply(player, command, &s.catalog)
            .unwrap()
            .created_id
            .unwrap_or(0)
    }

    fn review_member(s: &mut SandboxRuntime, ui: &mut UiState, mode: InputMode) {
        start_input(mode, ui, s);
        assert!(finish_input(mode, "2", ui, s).is_none());
        assert!(has_review(ui));
    }

    fn displayed_confirmation(s: &SandboxRuntime, ui: &mut UiState) -> CommunityAction {
        refresh_displayed(s, ui);
        bind_review_action(ui, CommunityAction::Confirm).expect("a current review was displayed")
    }

    #[test]
    fn removal_and_handover_require_explicit_review_and_confirmation() {
        for mode in [InputMode::RemoveMember, InputMode::TransferLeader] {
            let (mut s, mut ui, faction) = fixture();
            let before = s.state.clone();
            review_member(&mut s, &mut ui, mode);
            assert_eq!(s.state, before);
            let copy = describe(CommunityLabel::Review, &s, &ui);
            assert!(copy.contains("player #2"));
            assert!(copy.contains("River Commons"));
            assert!(copy.contains("Contributions are not refunded"));
            assert!(ui.communities.scroll_revision > 0);
            let confirmation = displayed_confirmation(&s, &mut ui);
            let command = act(confirmation, &mut ui, &mut s).unwrap();
            assert!(!has_review(&ui));
            assert!(
                act(confirmation, &mut ui, &mut s).is_none(),
                "one review can send only one command"
            );
            assert_eq!(s.state, before, "UI cannot mutate authority directly");
            apply(&mut s, 1, command);
            if mode == InputMode::RemoveMember {
                assert!(!s.state.factions[&faction].members.contains_key(&2));
            } else {
                assert_eq!(s.state.factions[&faction].leader, 2);
                assert_eq!(s.state.factions[&faction].members[&1], FactionRole::Builder);
            }
        }
    }

    #[test]
    fn cancel_and_changed_player_restore_selection_or_roster_cannot_submit_old_review() {
        for change in 0..5 {
            let (mut s, mut ui, faction) = fixture();
            review_member(&mut s, &mut ui, InputMode::TransferLeader);
            let confirmation = displayed_confirmation(&s, &mut ui);
            match change {
                0 => {
                    act(CommunityAction::Cancel, &mut ui, &mut s);
                }
                1 => {
                    s.restore_revision += 1;
                }
                2 => {
                    s.player_id = 2;
                }
                3 => {
                    let other = apply(
                        &mut s,
                        1,
                        Command::CreateFaction {
                            name: "Other community".into(),
                        },
                    );
                    ui.selected_faction = Some(other);
                }
                _ => {
                    apply(
                        &mut s,
                        1,
                        Command::SetFactionRole {
                            faction_id: faction,
                            player_id: 2,
                            role: FactionRole::Builder,
                        },
                    );
                }
            }
            let before = s.state.clone();
            assert!(unavailable(CommunityAction::Confirm, &s, &ui));
            assert!(act(confirmation, &mut ui, &mut s).is_none());
            assert_eq!(s.state, before);
            assert_eq!(s.state.factions[&faction].leader, 1);
        }
    }

    #[test]
    fn queued_confirmation_cannot_consume_a_replacement_review_or_reuse_a_reset_ui_token() {
        let (mut s, mut ui, faction) = fixture();
        review_member(&mut s, &mut ui, InputMode::TransferLeader);
        assert!(bind_review_action(&ui, CommunityAction::Confirm).is_none());
        let old_confirmation = displayed_confirmation(&s, &mut ui);
        act(CommunityAction::Cancel, &mut ui, &mut s);
        review_member(&mut s, &mut ui, InputMode::RemoveMember);
        assert!(bind_review_action(&ui, CommunityAction::Confirm).is_none());
        let before = s.state.clone();
        assert!(act(CommunityAction::Confirm, &mut ui, &mut s).is_none());
        assert!(has_review(&ui), "an unbound click cannot consume a review");
        let current_confirmation = displayed_confirmation(&s, &mut ui);
        assert!(unavailable(old_confirmation, &s, &ui));
        assert!(act(old_confirmation, &mut ui, &mut s).is_none());
        assert!(
            has_review(&ui),
            "a stale click preserves the current review"
        );
        assert!(!unavailable(current_confirmation, &s, &ui));
        let command = act(current_confirmation, &mut ui, &mut s).unwrap();
        assert!(
            matches!(command, Command::RemoveFactionMember { faction_id, player_id: 2 } if faction_id == faction)
        );
        assert_eq!(s.state, before);

        ui = UiState {
            selected_faction: Some(faction),
            ..Default::default()
        };
        review_member(&mut s, &mut ui, InputMode::TransferLeader);
        let reset_confirmation = displayed_confirmation(&s, &mut ui);
        assert!(act(old_confirmation, &mut ui, &mut s).is_none());
        assert!(act(current_confirmation, &mut ui, &mut s).is_none());
        assert!(has_review(&ui));
        assert!(
            matches!(act(reset_confirmation, &mut ui, &mut s), Some(Command::TransferFactionLeadership { faction_id, player_id: 2 }) if faction_id == faction)
        );
        assert_eq!(s.state, before);
    }

    #[test]
    fn hidden_community_review_cannot_bind_confirmation_until_the_panel_is_displayed() {
        let (mut s, mut ui, _) = fixture();
        review_member(&mut s, &mut ui, InputMode::TransferLeader);
        let before = s.state.clone();
        for (context_panel, panel) in [(true, Panel::Activities), (false, Panel::Factions)] {
            ui.context_panel = context_panel;
            ui.panel = panel;
            refresh_displayed(&s, &mut ui);
            assert!(bind_review_action(&ui, CommunityAction::Confirm).is_none());
            assert!(has_review(&ui));
        }
        ui.context_panel = true;
        ui.panel = Panel::Factions;
        refresh_displayed(&s, &mut ui);
        assert!(bind_review_action(&ui, CommunityAction::Confirm).is_some());
        ui.context_panel = false;
        refresh_displayed(&s, &mut ui);
        assert!(bind_review_action(&ui, CommunityAction::Confirm).is_none());
        assert_eq!(s.state, before);
    }

    #[test]
    fn member_entry_is_bound_to_its_faction_and_rejects_invalid_successors() {
        let (mut s, mut ui, faction) = fixture();
        for target in ["0", "-2", "1", "4", "999", "abc"] {
            start_input(InputMode::TransferLeader, &mut ui, &mut s);
            assert!(finish_input(InputMode::TransferLeader, target, &mut ui, &mut s).is_none());
            assert!(!has_review(&ui));
            assert_eq!(s.state.factions[&faction].leader, 1);
        }
        start_input(InputMode::RemoveMember, &mut ui, &mut s);
        s.restore_revision += 1;
        assert!(finish_input(InputMode::RemoveMember, "2", &mut ui, &mut s).is_none());
        assert!(!has_review(&ui));
        ui.editing_gps = false;
        s.player_id = 2;
        start_input(InputMode::RemoveMember, &mut ui, &mut s);
        assert!(!ui.editing_gps);
        assert!(s.last_message.contains("current leader"));
    }

    #[test]
    fn ordinary_member_can_review_leaving_but_current_leader_cannot() {
        let (mut s, mut ui, faction) = fixture();
        assert!(unavailable(CommunityAction::Leave, &s, &ui));
        assert!(act(CommunityAction::Leave, &mut ui, &mut s).is_none());
        assert!(!has_review(&ui));
        s.player_id = 2;
        assert!(!unavailable(CommunityAction::Leave, &s, &ui));
        assert!(unavailable(CommunityAction::TransferLeadership, &s, &ui));
        act(CommunityAction::Leave, &mut ui, &mut s);
        assert!(
            describe(CommunityLabel::Review, &s, &ui)
                .contains("Rejoining requires a new invitation")
        );
        let confirmation = displayed_confirmation(&s, &mut ui);
        let command = act(confirmation, &mut ui, &mut s).unwrap();
        assert!(matches!(command, Command::LeaveFaction { faction_id } if faction_id == faction));
        apply(&mut s, 2, command);
        assert_eq!(s.state.factions[&faction].leader, 1);
        assert!(!s.state.factions[&faction].members.contains_key(&2));
    }

    #[test]
    fn invitation_actions_use_the_named_display_and_never_fall_through_to_another_invite() {
        let (mut s, mut ui, _) = fixture();
        let first = apply(
            &mut s,
            3,
            Command::CreateFaction {
                name: "Orchard".into(),
            },
        );
        let second = apply(
            &mut s,
            4,
            Command::CreateFaction {
                name: "Harbor".into(),
            },
        );
        apply(
            &mut s,
            3,
            Command::InviteToFaction {
                faction_id: first,
                player_id: 1,
            },
        );
        apply(
            &mut s,
            4,
            Command::InviteToFaction {
                faction_id: second,
                player_id: 1,
            },
        );
        refresh_displayed(&s, &mut ui);
        assert!(describe(CommunityLabel::Invitation, &s, &ui).contains("Orchard"));
        assert!(
            matches!(accept_invitation(&mut s, &ui), Some(Command::AcceptFactionInvite { faction_id }) if faction_id == first)
        );
        apply(
            &mut s,
            3,
            Command::RevokeFactionInvite {
                faction_id: first,
                player_id: 1,
            },
        );
        assert!(accept_invitation(&mut s, &ui).is_none());
        assert!(act(CommunityAction::DeclineInvitation, &mut ui, &mut s).is_none());
        assert!(s.state.factions[&second].invites.contains(&1));
        refresh_displayed(&s, &mut ui);
        assert!(describe(CommunityLabel::Invitation, &s, &ui).contains("Harbor"));
        let command = act(CommunityAction::DeclineInvitation, &mut ui, &mut s).unwrap();
        assert!(
            matches!(command, Command::DeclineFactionInvite { faction_id } if faction_id == second)
        );
        assert!(
            !has_review(&ui),
            "declining an invitation needs no extra confirmation"
        );
    }

    #[test]
    fn sharing_review_names_permanent_property_and_allows_normal_crop_growth() {
        let (mut s, mut ui, faction) = fixture();
        let origin = s.state.players[&1].position;
        let plot = apply(
            &mut s,
            1,
            Command::ClaimPlot {
                center: origin,
                radius_m: 30.,
            },
        );
        let farm = apply(
            &mut s,
            1,
            Command::Plant {
                plot_id: plot,
                position: origin,
                crop: "wheat".into(),
            },
        );
        let before = s.state.clone();
        review_share_plot(&mut ui, &mut s);
        assert_eq!(s.state, before);
        let copy = describe(CommunityLabel::Review, &s, &ui);
        assert!(copy.contains(&format!("plot #{plot}")));
        assert!(copy.contains("no undo"));
        let confirmation = displayed_confirmation(&s, &mut ui);
        s.state.farms.get_mut(&farm).unwrap().growth_seconds += 1.;
        let command = act(confirmation, &mut ui, &mut s).unwrap();
        assert!(
            matches!(command, Command::TransferPlotToFaction { faction_id, plot_id } if faction_id == faction && plot_id == plot)
        );
        review_share_plot(&mut ui, &mut s);
        let confirmation = displayed_confirmation(&s, &mut ui);
        s.state.farms.get_mut(&farm).unwrap().owner = 2;
        assert!(
            act(confirmation, &mut ui, &mut s).is_none(),
            "ownership changes require a new review"
        );
    }

    #[test]
    fn roster_is_paged_and_current_role_explains_disabled_management() {
        let (mut s, mut ui, faction) = fixture();
        let origin = s.state.players[&1].position;
        for player in 5..=20 {
            s.state.add_player(player, origin).unwrap();
            apply(
                &mut s,
                1,
                Command::InviteToFaction {
                    faction_id: faction,
                    player_id: player,
                },
            );
        }
        let first = describe(CommunityLabel::Roster, &s, &ui);
        assert_eq!(first.lines().count(), ROSTER_PAGE_SIZE + 1);
        assert!(first.contains("page 1 of 3"));
        assert!(!unavailable(CommunityAction::NextRosterPage, &s, &ui));
        act(CommunityAction::NextRosterPage, &mut ui, &mut s);
        let second = describe(CommunityLabel::Roster, &s, &ui);
        assert!(second.contains("page 2 of 3"));
        assert_ne!(first, second);
        assert_eq!(ui.selected_faction, Some(faction));
        s.player_id = 2;
        assert!(summary(&s, &ui).contains("Ask the leader for building rights"));
        assert!(legacy_unavailable(Action::PromoteFaction, &s, &ui));
        assert!(unavailable(CommunityAction::RemoveMember, &s, &ui));
    }

    fn solo_fixture() -> (SandboxRuntime, UiState, u64) {
        let (mut s, ui, faction) = fixture();
        for player in [2, 3] {
            apply(
                &mut s,
                player,
                Command::LeaveFaction {
                    faction_id: faction,
                },
            );
        }
        (s, ui, faction)
    }

    #[test]
    fn unused_faction_dissolution_names_consequences_and_requires_fresh_confirmation() {
        let (mut s, mut ui, faction) = solo_fixture();
        apply(
            &mut s,
            1,
            Command::InviteToFaction {
                faction_id: faction,
                player_id: 2,
            },
        );
        let before = s.state.clone();
        assert!(!unavailable(CommunityAction::Dissolve, &s, &ui));
        assert!(act(CommunityAction::Dissolve, &mut ui, &mut s).is_none());
        assert_eq!(s.state, before);
        let copy = describe(CommunityLabel::Review, &s, &ui);
        for expected in [
            &format!("River Commons #{faction}"),
            "Dissolution is permanent",
            "Revokes 1 invitation",
            "No property, public stock or funds move; no refund",
            "authority rechecks",
            "nearby data cannot prove",
            "Cancel keeps the faction",
            "new ID",
        ] {
            assert!(
                copy.contains(expected),
                "missing review consequence: {expected}"
            );
        }
        assert!(ui.communities.scroll_revision > 0);
        assert!(act(CommunityAction::Cancel, &mut ui, &mut s).is_none());
        assert!(!has_review(&ui));
        assert_eq!(s.state, before);
        assert!(act(CommunityAction::Confirm, &mut ui, &mut s).is_none());

        act(CommunityAction::Dissolve, &mut ui, &mut s);
        let confirmation = displayed_confirmation(&s, &mut ui);
        let command = act(confirmation, &mut ui, &mut s).unwrap();
        assert!(
            matches!(command, Command::DissolveFaction { faction_id } if faction_id == faction)
        );
        assert_eq!(
            s.state, before,
            "confirmation prepares an intent, not authority"
        );
        apply(&mut s, 1, command);
        assert!(!s.state.factions.contains_key(&faction));
        assert!(act(confirmation, &mut ui, &mut s).is_none());
        assert!(unavailable(CommunityAction::Dissolve, &s, &ui));
    }

    #[test]
    fn dissolution_review_rejects_changed_identity_roster_restore_and_shared_property() {
        for change in 0..7 {
            let (mut s, mut ui, faction) = solo_fixture();
            let position = s.state.players[&1].position;
            let plot = apply(
                &mut s,
                1,
                Command::ClaimPlot {
                    center: position,
                    radius_m: 20.,
                },
            );
            let other = apply(
                &mut s,
                1,
                Command::CreateFaction {
                    name: "Different community".into(),
                },
            );
            act(CommunityAction::Dissolve, &mut ui, &mut s);
            assert!(has_review(&ui));
            let confirmation = displayed_confirmation(&s, &mut ui);
            match change {
                0 => s.player_id = 2,
                1 => s.restore_revision += 1,
                2 => ui.selected_faction = Some(other),
                3 => s.state.factions.get_mut(&faction).unwrap().name = "Changed name".into(),
                4 => {
                    apply(
                        &mut s,
                        1,
                        Command::InviteToFaction {
                            faction_id: faction,
                            player_id: 2,
                        },
                    );
                }
                5 => {
                    apply(
                        &mut s,
                        1,
                        Command::TransferPlotToFaction {
                            plot_id: plot,
                            faction_id: faction,
                        },
                    );
                }
                6 => s.state.set_player_dormant(1, true).unwrap(),
                _ => unreachable!(),
            }
            let before = s.state.clone();
            assert!(unavailable(CommunityAction::Confirm, &s, &ui));
            assert!(describe(CommunityLabel::Review, &s, &ui).contains("Review expired"));
            assert!(act(confirmation, &mut ui, &mut s).is_none());
            assert_eq!(s.state, before);
            assert!(!has_review(&ui));
        }
    }

    #[test]
    fn filtered_remote_dissolution_review_keeps_whole_world_eligibility_on_authority() {
        let (mut authority, _, faction) = solo_fixture();
        let position = authority.state.players[&1].position;
        let distant = GeoPosition::new(50., 14., 100.).unwrap();
        authority.state.set_player_position(1, distant).unwrap();
        let plot = apply(
            &mut authority,
            1,
            Command::ClaimPlot {
                center: distant,
                radius_m: 20.,
            },
        );
        apply(
            &mut authority,
            1,
            Command::TransferPlotToFaction {
                plot_id: plot,
                faction_id: faction,
            },
        );
        authority.state.set_player_position(1, position).unwrap();
        let view = authority.state.snapshot_for(1, 100.).unwrap();
        assert!(view.plots.is_empty());
        assert!(view.factions.iter().any(|f| f.id == faction));
        let mut replica = SandboxRuntime::new(position);
        replica.state.players.insert(1, view.player);
        replica.state.factions = view.factions.into_iter().map(|f| (f.id, f)).collect();
        let mut ui = UiState {
            selected_faction: Some(faction),
            panel: Panel::Factions,
            ..Default::default()
        };
        act(CommunityAction::Dissolve, &mut ui, &mut replica);
        assert!(has_review(&ui));
        let copy = describe(CommunityLabel::Review, &replica, &ui);
        assert!(copy.contains("authority rechecks"));
        assert!(copy.contains("nearby data cannot prove"));
        let confirmation = displayed_confirmation(&replica, &mut ui);
        let command = act(confirmation, &mut ui, &mut replica).unwrap();
        let before = authority.state.clone();
        assert!(
            authority
                .state
                .apply(1, command, &authority.catalog)
                .is_err()
        );
        assert_eq!(
            authority.state, before,
            "unseen remote property remains protected"
        );
    }
}
