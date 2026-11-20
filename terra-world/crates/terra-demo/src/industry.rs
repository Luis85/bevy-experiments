//! Industry actions keep the facility and desired operation shown at activation.
use crate::{
    interface::{Action, Panel, UiState},
    reviewed_action::{NativeContext, next_token},
};
use terra_bevy::{SandboxRuntime, rules::*};
use terra_core::GeoPosition;

#[derive(Default)]
pub(crate) struct IndustryUi {
    displayed: Option<IndustryReview>,
}

#[derive(Clone)]
struct IndustryReview {
    token: u64,
    context: IndustryContext,
}

#[derive(Clone, PartialEq)]
struct IndustryContext {
    actor: NativeContext,
    selection: Option<u64>,
    id: u64,
    name: String,
    position: GeoPosition,
    owner: Option<u64>,
    enabled: bool,
    kind: ProductionKind,
    inputs: Inventory,
    outputs: Inventory,
    cycle_seconds: f64,
    source_feature: Option<String>,
}

pub(crate) fn selected<'a>(s: &'a SandboxRuntime, ui: &UiState) -> Option<&'a ProductionSite> {
    match ui.selected_site {
        Some(id) => s.state.living.producers.get(&id),
        None => s.state.living.producers.values().next(),
    }
}

fn capture(s: &SandboxRuntime, ui: &UiState) -> Option<IndustryContext> {
    if !ui.context_panel || ui.panel != Panel::Industry {
        return None;
    }
    let site = selected(s, ui)?;
    Some(IndustryContext {
        actor: NativeContext::capture(s)?,
        selection: ui.selected_site,
        id: site.id,
        name: site.name.clone(),
        position: site.position,
        owner: site.owner,
        enabled: site.enabled,
        kind: site.kind,
        inputs: site.inputs.clone(),
        outputs: site.outputs.clone(),
        cycle_seconds: site.cycle_seconds,
        source_feature: site.source_feature.clone(),
    })
}

pub(crate) fn refresh_displayed(s: &SandboxRuntime, ui: &mut UiState) {
    let current = capture(s, ui);
    if current
        .as_ref()
        .zip(ui.industry.displayed.as_ref())
        .is_some_and(|(current, shown)| current == &shown.context)
    {
        return;
    }
    ui.industry.displayed =
        current.and_then(|context| next_token().map(|token| IndustryReview { token, context }));
}

pub(crate) fn bind(ui: &UiState, travel: bool) -> Option<Action> {
    ui.industry
        .displayed
        .as_ref()
        .map(|shown| Action::IndustryReviewed {
            token: shown.token,
            travel,
        })
}

pub(crate) fn reviewed_target(
    s: &mut SandboxRuntime,
    ui: &mut UiState,
    token: u64,
    travel: bool,
) -> Option<(u64, GeoPosition, bool)> {
    let Some(shown) = ui.industry.displayed.as_ref() else {
        s.last_message =
            "The displayed Industry operation expired. Review the named facility again.".into();
        return None;
    };
    let current = capture(s, ui);
    if shown.token != token || current.as_ref() != Some(&shown.context) {
        s.last_message = "The displayed facility, operation or active actor changed. Review Industry before choosing it again.".into();
        return None;
    }
    if !travel && let Some(problem) = problem(s, ui) {
        s.last_message = problem;
        return None;
    }
    let result = (
        shown.context.id,
        shown.context.position,
        !shown.context.enabled,
    );
    ui.industry.displayed = None;
    Some(result)
}

pub(crate) fn problem(s: &SandboxRuntime, ui: &UiState) -> Option<String> {
    let Some(site) = selected(s, ui) else {
        return Some(match ui.selected_site {
            Some(id) => format!(
                "Facility #{id} is unavailable in this view. Choose Next facility to review another."
            ),
            None => "No production facilities are loaded here. Explore another area.".into(),
        });
    };
    if NativeContext::capture(s).is_none() {
        return Some("Resume an owned active actor before managing production.".into());
    }
    if site.owner != Some(s.player_id) {
        return Some(format!(
            "{} is not your facility. Only its owner can pause or resume production.",
            site.name
        ));
    }
    let position = s.state.players[&s.player_id].position;
    let distance = position
        .surface_distance_m(site.position)
        .hypot(position.altitude_m - site.position.altitude_m);
    (distance > INTERACTION_REACH_M).then(|| format!(
        "Approach {} to within {INTERACTION_REACH_M:.0} m to manage production ({distance:.0} m away).", site.name
    ))
}

pub(crate) fn operation_label(s: &SandboxRuntime, ui: &UiState) -> String {
    selected(s, ui)
        .map(|site| {
            format!(
                "{} {}",
                if site.enabled { "Pause" } else { "Resume" },
                site.name
            )
        })
        .unwrap_or_else(|| "Production unavailable".into())
}
