//! Named, one-use consent for native structure damage. No serialized review state.
use crate::{
    interface::{Action, Panel, UiState},
    reviewed_action::{NativeContext, next_token},
};
use terra_bevy::{SandboxRuntime, rules::*};

const DAMAGE: u32 = 25;

#[derive(Default)]
pub(crate) struct BuildingReviewUi {
    displayed_target: Option<DamageReview>,
    review: Option<DamageReview>,
    displayed_confirmation: Option<u64>,
    pub(crate) scroll_revision: u64,
}

#[derive(Clone)]
struct DamageReview {
    token: u64,
    actor: NativeContext,
    structure: Structure,
}

fn in_reach(s: &SandboxRuntime, structure: &Structure) -> bool {
    s.state.players.get(&s.player_id).is_some_and(|player| {
        player
            .position
            .surface_distance_m(structure.position)
            .hypot(player.position.altitude_m - structure.position.altitude_m)
            <= INTERACTION_REACH_M
    })
}

fn candidate(s: &SandboxRuntime) -> Option<&Structure> {
    let position = s.state.players.get(&s.player_id)?.position;
    s.state
        .structures
        .values()
        .filter(|structure| {
            s.state
                .may_interact(s.player_id, structure.faction_owner, structure.owner)
        })
        .min_by(|a, b| {
            a.position
                .surface_distance_m(position)
                .hypot(a.position.altitude_m - position.altitude_m)
                .total_cmp(
                    &b.position
                        .surface_distance_m(position)
                        .hypot(b.position.altitude_m - position.altitude_m),
                )
                .then(a.id.cmp(&b.id))
        })
}

impl DamageReview {
    fn is_current(&self, s: &SandboxRuntime) -> bool {
        self.actor.is_current(s)
            && s.state.structures.get(&self.structure.id) == Some(&self.structure)
            && s.state.may_interact(
                s.player_id,
                self.structure.faction_owner,
                self.structure.owner,
            )
            && in_reach(s, &self.structure)
    }
}

pub(crate) fn refresh_displayed(s: &SandboxRuntime, ui: &mut UiState) {
    let visible = ui.context_panel && ui.panel == Panel::Build;
    let target = visible.then(|| candidate(s)).flatten();
    let actor = NativeContext::capture(s);
    if !ui
        .building_review
        .displayed_target
        .as_ref()
        .is_some_and(|shown| {
            Some(&shown.structure) == target && Some(&shown.actor) == actor.as_ref()
        })
    {
        ui.building_review.displayed_target = target.zip(actor).and_then(|(structure, actor)| {
            next_token().map(|token| DamageReview {
                token,
                actor,
                structure: structure.clone(),
            })
        });
    }
    ui.building_review.displayed_confirmation = visible
        .then(|| {
            ui.building_review
                .review
                .as_ref()
                .filter(|review| review.is_current(s))
                .map(|review| review.token)
        })
        .flatten();
}

pub(crate) fn bind(ui: &UiState, confirm: bool) -> Option<Action> {
    if confirm {
        ui.building_review
            .displayed_confirmation
            .map(Action::ConfirmDamageReviewed)
    } else {
        ui.building_review
            .displayed_target
            .as_ref()
            .map(|shown| Action::ReviewDamage(shown.token))
    }
}

pub(crate) fn begin(token: u64, ui: &mut UiState, s: &mut SandboxRuntime) {
    let Some(shown) = ui
        .building_review
        .displayed_target
        .as_ref()
        .filter(|shown| {
            shown.token == token
                && shown.is_current(s)
                && candidate(s) == Some(&shown.structure)
                && ui.context_panel
                && ui.panel == Panel::Build
        })
    else {
        s.last_message = "The displayed structure or active actor changed, or is out of reach. Review the named target in Build again.".into();
        return;
    };
    let Some(review_token) = next_token() else {
        return;
    };
    let mut review = shown.clone();
    review.token = review_token;
    ui.building_review.review = Some(review);
    ui.building_review.displayed_confirmation = None;
    ui.building_review.scroll_revision = review_token;
    s.last_message =
        "Review the named structure and health change in Build, then confirm or cancel.".into();
}

pub(crate) fn confirm(token: u64, ui: &mut UiState, s: &mut SandboxRuntime) -> Option<Command> {
    if ui
        .building_review
        .review
        .as_ref()
        .is_none_or(|review| review.token != token)
    {
        s.last_message = "That confirmation belongs to an earlier building review. Review the current target before confirming.".into();
        return None;
    }
    let review = ui.building_review.review.take()?;
    ui.building_review.displayed_confirmation = None;
    if !ui.context_panel || ui.panel != Panel::Build || !review.is_current(s) {
        s.last_message = "Building review expired: health, ownership, active actor or reach changed. Review again before damaging it.".into();
        return None;
    }
    Some(Command::Damage {
        structure_id: review.structure.id,
        amount: DAMAGE,
    })
}

pub(crate) fn cancel(ui: &mut UiState, s: &mut SandboxRuntime) {
    ui.building_review.review = None;
    ui.building_review.displayed_confirmation = None;
    s.last_message = "Building damage canceled. No structure changed.".into();
}

pub(crate) fn has_review(ui: &UiState) -> bool {
    ui.building_review.review.is_some()
}

pub(crate) fn unavailable(s: &SandboxRuntime, ui: &UiState, confirm: bool) -> bool {
    if confirm {
        ui.building_review
            .review
            .as_ref()
            .is_none_or(|review| !review.is_current(s))
    } else {
        NativeContext::capture(s).is_none()
            || candidate(s).is_none_or(|structure| !in_reach(s, structure))
    }
}

fn name(structure: &Structure) -> String {
    format!(
        "{} #{}",
        structure.blueprint.replace('_', " "),
        structure.id
    )
}

pub(crate) fn target_label(s: &SandboxRuntime, ui: &UiState) -> String {
    let Some(shown) = ui.building_review.displayed_target.as_ref() else {
        return "No permitted structure is available. Build a structure or obtain building permission first.".into();
    };
    let structure = &shown.structure;
    let mut text = format!(
        "Target: {} · health {}/{}",
        name(structure),
        structure.hitpoints,
        structure.max_hitpoints
    );
    if !in_reach(s, structure) {
        text.push_str(&format!(
            "\nApproach within {INTERACTION_REACH_M:.0} m to review damage."
        ));
    }
    text
}

pub(crate) fn review_label(s: &SandboxRuntime, ui: &UiState) -> String {
    let Some(review) = &ui.building_review.review else {
        return String::new();
    };
    if !review.is_current(s) {
        return "Review expired. The structure, health, ownership, actor or reach changed. Cancel and review the target again.".into();
    }
    let structure = &review.structure;
    let remaining = structure.hitpoints.saturating_sub(DAMAGE);
    let consequence = if remaining == 0 {
        "This destroys the structure and clears every home set here. Materials are not refunded."
    } else {
        "This permanently reduces its health. Materials are not refunded."
    };
    format!(
        "Damage {}?\nHealth: {} → {remaining} (−{}).\n{consequence}\nCancel keeps the structure unchanged.",
        name(structure),
        structure.hitpoints,
        DAMAGE.min(structure.hitpoints)
    )
}
