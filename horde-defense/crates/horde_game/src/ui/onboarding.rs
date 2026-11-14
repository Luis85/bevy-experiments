//! One-time unlock callouts (UX-16): "New: X - find it in <place>" under the
//! camp row on Camp and Upgrades, never in a battle or on Results. The seen-set
//! is presentation memory in `Settings::seen_callouts` (ruling S4): it is
//! written with every save, a pre-pass file (None) is seeded with every
//! callout its journey already unlocked, and a New game starts it empty.
//! Unlock rules are read from core; nothing here mutates the game.
use super::{Screen, Tab, UiCtx, chrome};
use crate::{
    art::{AMBER, PANEL},
    settings::{self, Action, BindingId, ExtraAction, Settings},
};
use bevy_egui::egui::{self, Stroke};
use horde_core::{
    content::StructureKind,
    quality::Quality,
    sim::Game,
    skill_catalog::{CampaignFeature, unlocks},
};
use std::collections::BTreeSet;

/// One unlock announcement. `id` is saved in settings.json, so it never
/// changes and the table is append-only. Feature ids are "feature:" +
/// `CampaignFeature::id()`; the others are "<area>:<name>".
pub(super) struct Callout {
    pub(super) id: &'static str,
    /// The line, with the player's own keys.
    text: fn(&Settings) -> String,
    /// "Show me": opens the place. None when the place is the battlefield.
    open: Option<fn(&mut UiCtx)>,
    /// Whether the journey has what the callout announces (a core read).
    unlocked: fn(&Game) -> bool,
    /// The place the callout points at: arriving there marks it seen.
    place: fn(Screen, Tab) -> bool,
}

fn build_key(settings: &Settings) -> &str {
    settings::binding_text(settings.binding(BindingId::Core(Action::Build)))
}
fn cycle_key(settings: &Settings) -> &str {
    settings::binding_text(settings.binding(BindingId::Extra(ExtraAction::CycleAttack)))
}
fn in_battle(screen: Screen, _: Tab) -> bool {
    screen == Screen::Battle
}
fn on_operations(screen: Screen, tab: Tab) -> bool {
    screen == Screen::Camp && tab == Tab::Operations
}
fn open_operations(f: &mut UiCtx) {
    f.open_preparation(Tab::Operations);
}
fn has_blueprint(game: &Game, kind: StructureKind) -> bool {
    game.campaign().blueprints[kind]
}
fn blueprint_text(settings: &Settings, kind: StructureKind) -> String {
    format!(
        "New: {} blueprint - press {} in battle to build it",
        kind.label(),
        build_key(settings)
    )
}
fn ordnance_text(settings: &Settings, name: &str) -> String {
    format!(
        "New: {name} - switch attacks with {} in battle",
        cycle_key(settings)
    )
}

/// Every callout, in the order they are shown when several are due.
pub(super) const CALLOUTS: &[Callout] = &[
    Callout {
        id: "feature:construction",
        text: |s| format!("New: Build defenses - press {} in battle", build_key(s)),
        open: None,
        unlocked: |g| g.campaign().has_feature(CampaignFeature::Construction),
        place: in_battle,
    },
    Callout {
        id: "feature:companion",
        text: |_| "New: Mara - find her in Camp home".into(),
        open: Some(|f| f.open_preparation(Tab::Survivor)),
        unlocked: |g| g.campaign().has_feature(CampaignFeature::Companion),
        place: |screen, tab| screen == Screen::Camp && tab == Tab::Survivor,
    },
    Callout {
        id: "feature:recruitment",
        text: |_| "New: Recruiting - liberation Results and Camp home".into(),
        open: Some(|f| f.open_camp_home(None)),
        unlocked: |g| g.campaign().has_feature(CampaignFeature::Recruitment),
        place: |screen, tab| screen == Screen::Camp && tab == Tab::Home,
    },
    Callout {
        id: "feature:parallel",
        text: |_| "New: Expeditions - Camp home › Operations".into(),
        open: Some(open_operations),
        unlocked: |g| g.campaign().has_feature(CampaignFeature::Parallel),
        place: on_operations,
    },
    Callout {
        id: "feature:operations",
        text: |_| "New: Country operations - Camp home › Operations".into(),
        open: Some(open_operations),
        unlocked: |g| g.campaign().has_feature(CampaignFeature::Operations),
        place: on_operations,
    },
    Callout {
        id: "blueprint:trap",
        text: |s| blueprint_text(s, StructureKind::Trap),
        open: None,
        unlocked: |g| has_blueprint(g, StructureKind::Trap),
        place: in_battle,
    },
    Callout {
        id: "blueprint:wall",
        text: |s| blueprint_text(s, StructureKind::Wall),
        open: None,
        unlocked: |g| has_blueprint(g, StructureKind::Wall),
        place: in_battle,
    },
    Callout {
        id: "blueprint:turret",
        text: |s| blueprint_text(s, StructureKind::Turret),
        open: None,
        unlocked: |g| has_blueprint(g, StructureKind::Turret),
        place: in_battle,
    },
    Callout {
        id: "ordnance:grenade",
        text: |s| ordnance_text(s, "Grenades"),
        open: None,
        unlocked: |g| g.campaign().modifiers().unlocked(unlocks::GRENADE),
        place: in_battle,
    },
    Callout {
        id: "ordnance:molotov",
        text: |s| ordnance_text(s, "Molotovs"),
        open: None,
        unlocked: |g| g.campaign().modifiers().unlocked(unlocks::MOLOTOV),
        place: in_battle,
    },
    Callout {
        id: "defenders:takeover",
        text: |_| "New: Recruits can defend - choose on Results or Briefing".into(),
        open: Some(|f| f.open_briefing()),
        // A recruit is in camp: the picker now has someone to choose.
        unlocked: |g| !g.strategy().roster.is_empty(),
        place: |screen, tab| screen == Screen::Camp && tab == Tab::Briefing,
    },
    Callout {
        id: "loot:auto-sell",
        text: |_| "New: Auto-sell - set a rule in Equipment".into(),
        open: Some(|f| f.open_preparation(Tab::Armory)),
        // The first drop of a rolled quality an auto-sell rule can keep.
        unlocked: |g| {
            g.campaign()
                .items
                .iter()
                .any(|item| item.quality.is_some_and(|q| q >= Quality::Uncommon))
        },
        place: |screen, tab| screen == Screen::Camp && tab == Tab::Armory,
    },
];

/// The ids of every callout `game` has already unlocked.
pub(super) fn unlocked_ids(game: &Game) -> BTreeSet<String> {
    CALLOUTS
        .iter()
        .filter(|callout| (callout.unlocked)(game))
        .map(|callout| callout.id.to_owned())
        .collect()
}

/// A settings file from before the callouts (None) takes every callout the
/// loaded journey already unlocked, so an old profile is not re-taught. A
/// seeded file is left alone. Runs once at profile load.
pub(super) fn seed_seen_callouts(settings: &mut Settings, game: &Game) {
    if settings.seen_callouts.is_none() {
        settings.seen_callouts = Some(unlocked_ids(game));
    }
}

/// A staged scene (review fixture or practice scenario) shows its screen,
/// not what its staging unlocked.
pub(super) fn mark_unlocked_seen(settings: &mut Settings, game: &Game) {
    settings
        .seen_callouts
        .get_or_insert_with(BTreeSet::new)
        .extend(unlocked_ids(game));
}

/// The callout due now: the first unlocked one not yet seen. None before
/// the seen-set is seeded.
pub(super) fn due(settings: &Settings, game: &Game) -> Option<&'static Callout> {
    let seen = settings.seen_callouts.as_ref()?;
    CALLOUTS
        .iter()
        .find(|callout| !seen.contains(callout.id) && (callout.unlocked)(game))
}

/// The line `callout` shows.
pub(super) fn text(callout: &Callout, settings: &Settings) -> String {
    (callout.text)(settings)
}

fn mark_seen(settings: &mut Settings, id: &str) {
    if let Some(seen) = settings.seen_callouts.as_mut() {
        seen.insert(id.to_owned());
    }
}

/// Arriving at a place marks its unlocked callouts seen, shown or not
/// (hints may be hidden). Checked when the screen or camp tab changes, not
/// every frame.
pub(super) fn visit(f: &mut UiCtx) {
    let place = (f.screen, f.tab);
    if f.nav.onboarding_place == Some(place) {
        return;
    }
    f.nav.onboarding_place = Some(place);
    let Some(seen) = f.settings.seen_callouts.as_ref() else {
        return;
    };
    let arrived: Vec<&'static str> = CALLOUTS
        .iter()
        .filter(|callout| {
            (callout.place)(place.0, place.1)
                && !seen.contains(callout.id)
                && (callout.unlocked)(f.game)
        })
        .map(|callout| callout.id)
        .collect();
    for id in arrived {
        mark_seen(f.settings, id);
    }
}

/// The callout row under the camp row: one callout at a time with "Show
/// me" and "Got it". Hidden, and not marked, while hints are off. It is
/// static: there is no motion for reduced effects to remove.
pub(super) fn callout_row(root: &mut egui::Ui, f: &mut UiCtx) {
    if !f.settings.show_guidance || !matches!(f.screen, Screen::Camp | Screen::Upgrades) {
        return;
    }
    let Some(callout) = due(f.settings, f.game) else {
        return;
    };
    let line = text(callout, f.settings);
    egui::Panel::top("onboarding_callout")
        .frame(
            egui::Frame::new()
                .fill(PANEL)
                .stroke(Stroke::new(1.0, AMBER.gamma_multiply(0.6)))
                .inner_margin(egui::Margin::symmetric(18, 5)),
        )
        .show(root, |ui| {
            chrome::row(ui, |ui| {
                ui.colored_label(AMBER, line);
                // "Show me" only leads somewhere else.
                if let Some(open) = callout.open
                    && !(callout.place)(f.screen, f.tab)
                    && ui.button("Show me").clicked()
                {
                    open(f);
                    mark_seen(f.settings, callout.id);
                }
                if ui.button("Got it").clicked() {
                    mark_seen(f.settings, callout.id);
                }
            });
        });
}
