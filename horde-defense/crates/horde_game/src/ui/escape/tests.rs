//! The pure Escape decision: the Screen x Tab x overlay matrix (G2.5, D18)
//! and the properties every context keeps.
use super::*;
use EscapeAction as A;

/// A started, active journey on `screen` with no overlay, nothing pending and
/// the default bindings (Pause is Escape).
fn on(screen: Screen) -> EscapeContext {
    EscapeContext {
        screen,
        tab: Tab::Home,
        overlay: Overlay::None,
        settled_report: false,
        journey_active: true,
        journey_started: true,
        attempt_live: false,
        durable_pending: false,
        draft_dirty: false,
        pause_is_escape: true,
        construction: false,
    }
}
fn camp(tab: Tab) -> EscapeContext {
    EscapeContext {
        tab,
        ..on(Screen::Camp)
    }
}
fn with(overlay: Overlay, cx: EscapeContext) -> EscapeContext {
    EscapeContext { overlay, ..cx }
}
fn pending(cx: EscapeContext) -> EscapeContext {
    EscapeContext {
        durable_pending: true,
        ..cx
    }
}
fn building(cx: EscapeContext) -> EscapeContext {
    EscapeContext {
        construction: true,
        ..cx
    }
}
fn pause_on_p(cx: EscapeContext) -> EscapeContext {
    EscapeContext {
        pause_is_escape: false,
        ..cx
    }
}
/// Settings with an unapplied difficulty draft.
fn dirty_settings() -> EscapeContext {
    EscapeContext {
        draft_dirty: true,
        ..on(Screen::Settings)
    }
}
/// Camp home after a settled defense, with no main defense live.
fn home_after_report() -> EscapeContext {
    EscapeContext {
        settled_report: true,
        ..camp(Tab::Home)
    }
}

const QUIT: ModalKind = ModalKind::Quit;
const RETURN: ModalKind = ModalKind::ReturnToMenu;
const DRAFT: ModalKind = ModalKind::DraftWarning;

const TABS: [Tab; 9] = [
    Tab::Home,
    Tab::World,
    Tab::Briefing,
    Tab::Character,
    Tab::Survivor,
    Tab::Armory,
    Tab::Skills,
    Tab::Engineering,
    Tab::Operations,
];
const OVERLAYS: [Overlay; 12] = [
    Overlay::Rebind,
    Overlay::TextFocus,
    Overlay::AppModal(ModalKind::Quit),
    Overlay::AppModal(ModalKind::ReturnToMenu),
    Overlay::AppModal(ModalKind::Storage),
    Overlay::AppModal(ModalKind::DraftWarning),
    Overlay::NewJourneyModal,
    Overlay::Details,
    Overlay::Popup,
    Overlay::Observer,
    Overlay::ArmoryConfirm,
    Overlay::None,
];

#[test]
fn matrix_table() {
    let rows: Vec<(&str, EscapeContext, EscapeAction)> = vec![
        // Menu and survivor selection.
        ("menu asks to quit", on(Screen::Menu), A::OpenModal(QUIT)),
        (
            "menu with no journey asks to quit",
            EscapeContext {
                journey_started: false,
                journey_active: false,
                ..on(Screen::Menu)
            },
            A::OpenModal(QUIT),
        ),
        (
            "quit modal closes",
            with(Overlay::AppModal(QUIT), on(Screen::Menu)),
            A::CloseModal,
        ),
        (
            "storage modal over a battle closes",
            with(Overlay::AppModal(ModalKind::Storage), on(Screen::Battle)),
            A::CloseModal,
        ),
        (
            "storage modal closes while a save is pending",
            pending(with(
                Overlay::AppModal(ModalKind::Storage),
                on(Screen::Menu),
            )),
            A::CloseModal,
        ),
        (
            "new-journey confirmation closes",
            with(Overlay::NewJourneyModal, on(Screen::Menu)),
            A::CloseNewJourney,
        ),
        (
            "new-journey confirmation stays while the reset runs",
            pending(with(Overlay::NewJourneyModal, on(Screen::Menu))),
            A::Ignore,
        ),
        (
            "menu waits for a pending quit save",
            pending(on(Screen::Menu)),
            A::Ignore,
        ),
        (
            "survivor selection goes back to the menu",
            on(Screen::SurvivorSelection),
            A::Go(Screen::Menu),
        ),
        (
            "survivor selection popup is egui's",
            with(Overlay::Popup, on(Screen::SurvivorSelection)),
            A::LeaveToEgui,
        ),
        // Camp before the first start.
        (
            "world map before the first start goes to the menu",
            EscapeContext {
                journey_started: false,
                journey_active: false,
                ..camp(Tab::World)
            },
            A::Go(Screen::Menu),
        ),
        (
            "country search keeps its Escape before the first start",
            with(
                Overlay::TextFocus,
                EscapeContext {
                    journey_started: false,
                    ..camp(Tab::World)
                },
            ),
            A::LeaveToEgui,
        ),
        // Camp home.
        (
            "camp home with no report asks to return to the menu",
            camp(Tab::Home),
            A::OpenModal(RETURN),
        ),
        (
            "camp home after a report goes to Results",
            home_after_report(),
            A::Go(Screen::Results),
        ),
        (
            "camp home with a live defense asks to return to the menu",
            EscapeContext {
                attempt_live: true,
                ..home_after_report()
            },
            A::OpenModal(RETURN),
        ),
        (
            "camp home after the journey ended asks to return to the menu",
            EscapeContext {
                journey_active: false,
                ..home_after_report()
            },
            A::OpenModal(RETURN),
        ),
        (
            "return modal on camp home closes",
            with(Overlay::AppModal(RETURN), home_after_report()),
            A::CloseModal,
        ),
        (
            "camp home waits for a pending menu save",
            pending(home_after_report()),
            A::Ignore,
        ),
        (
            "camp home details window closes",
            with(Overlay::Details, camp(Tab::Home)),
            A::CloseDetails,
        ),
        // Camp tabs and drill-downs go back to Camp home.
        ("world map", camp(Tab::World), A::GoTab(Tab::Home)),
        ("briefing", camp(Tab::Briefing), A::GoTab(Tab::Home)),
        ("character", camp(Tab::Character), A::GoTab(Tab::Home)),
        ("equipment", camp(Tab::Armory), A::GoTab(Tab::Home)),
        ("mara drill-down", camp(Tab::Survivor), A::GoTab(Tab::Home)),
        (
            "operations drill-down",
            camp(Tab::Operations),
            A::GoTab(Tab::Home),
        ),
        ("skills redirect", camp(Tab::Skills), A::GoTab(Tab::Home)),
        (
            "fortifications redirect",
            camp(Tab::Engineering),
            A::GoTab(Tab::Home),
        ),
        (
            "observer window closes before the tab",
            with(Overlay::Observer, camp(Tab::Operations)),
            A::CloseObserver,
        ),
        (
            "armory sale confirmation cancels before the tab",
            with(Overlay::ArmoryConfirm, camp(Tab::Armory)),
            A::CancelArmoryConfirm,
        ),
        (
            "an armory filter popup is egui's",
            with(Overlay::Popup, camp(Tab::Armory)),
            A::LeaveToEgui,
        ),
        (
            "details over the observer closes first",
            with(Overlay::Details, camp(Tab::Operations)),
            A::CloseDetails,
        ),
        (
            "a camp tab waits for a pending save",
            pending(camp(Tab::Briefing)),
            A::Ignore,
        ),
        // Battle (decision 18).
        (
            "battle passes Escape to Pause",
            on(Screen::Battle),
            A::PassToPause,
        ),
        (
            "battle ignores Escape when Pause is rebound",
            pause_on_p(on(Screen::Battle)),
            A::Ignore,
        ),
        (
            "construction closes without pausing",
            building(on(Screen::Battle)),
            A::CloseConstruction,
        ),
        (
            "construction closes when Pause is rebound",
            pause_on_p(building(on(Screen::Battle))),
            A::CloseConstruction,
        ),
        (
            "the management menu is egui's",
            with(Overlay::Popup, building(on(Screen::Battle))),
            A::LeaveToEgui,
        ),
        (
            "details over construction close first",
            with(Overlay::Details, building(on(Screen::Battle))),
            A::CloseDetails,
        ),
        (
            "battle still pauses while a save is pending",
            pending(on(Screen::Battle)),
            A::PassToPause,
        ),
        // Results.
        (
            "results asks to return to the menu",
            EscapeContext {
                settled_report: true,
                ..on(Screen::Results)
            },
            A::OpenModal(RETURN),
        ),
        (
            "return modal on results closes",
            with(Overlay::AppModal(RETURN), on(Screen::Results)),
            A::CloseModal,
        ),
        (
            "the results More menu is egui's",
            with(Overlay::Popup, on(Screen::Results)),
            A::LeaveToEgui,
        ),
        (
            "tree search keeps its Escape",
            with(Overlay::TextFocus, on(Screen::Results)),
            A::LeaveToEgui,
        ),
        // History and settings.
        ("history closes", on(Screen::History), A::CloseHistory),
        (
            "history waits for a pending save",
            pending(on(Screen::History)),
            A::Ignore,
        ),
        (
            "settings save and return",
            on(Screen::Settings),
            A::SettingsBack,
        ),
        (
            "settings rebind cancels",
            with(Overlay::Rebind, on(Screen::Settings)),
            A::CancelRebind,
        ),
        (
            "settings rebind cancels while a save is pending",
            pending(with(Overlay::Rebind, on(Screen::Settings))),
            A::CancelRebind,
        ),
        (
            "settings waits for its pending save",
            pending(on(Screen::Settings)),
            A::Ignore,
        ),
        (
            "a settings dropdown is egui's",
            with(Overlay::Popup, on(Screen::Settings)),
            A::LeaveToEgui,
        ),
        // The draft warning (W3-07, UX-09).
        (
            "settings with an unapplied draft opens the draft warning",
            dirty_settings(),
            A::OpenModal(DRAFT),
        ),
        (
            "the draft warning closes: keep editing",
            with(Overlay::AppModal(DRAFT), dirty_settings()),
            A::CloseModal,
        ),
        (
            "the draft warning closes while its save is pending",
            pending(with(Overlay::AppModal(DRAFT), dirty_settings())),
            A::CloseModal,
        ),
        (
            "a rebind cancels before the draft warning opens",
            with(Overlay::Rebind, dirty_settings()),
            A::CancelRebind,
        ),
        (
            "a dropdown over an unapplied draft is egui's",
            with(Overlay::Popup, dirty_settings()),
            A::LeaveToEgui,
        ),
        (
            "an unapplied draft waits for a pending save",
            pending(dirty_settings()),
            A::Ignore,
        ),
        (
            "an unapplied draft matters only on Settings",
            EscapeContext {
                draft_dirty: true,
                ..camp(Tab::Briefing)
            },
            A::GoTab(Tab::Home),
        ),
    ];
    assert!(rows.len() >= 52, "{} rows", rows.len());
    let failures: Vec<String> = rows
        .iter()
        .filter_map(|(name, cx, expected)| {
            let got = escape_action(cx);
            (got != *expected).then(|| format!("{name}: expected {expected:?}, got {got:?}"))
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Every context: each screen, tab, overlay and campaign flag.
fn every_context() -> Vec<EscapeContext> {
    let mut all = Vec::new();
    for screen in Screen::ALL {
        for tab in TABS {
            for overlay in OVERLAYS {
                for bits in 0u16..256 {
                    let bit = |n: u16| bits & (1 << n) != 0;
                    all.push(EscapeContext {
                        screen,
                        tab,
                        overlay,
                        settled_report: bit(0),
                        journey_active: bit(1),
                        journey_started: bit(2),
                        attempt_live: bit(3),
                        durable_pending: bit(4),
                        pause_is_escape: bit(5),
                        construction: bit(6),
                        draft_dirty: bit(7),
                    });
                }
            }
        }
    }
    all
}

/// The context one press of `action` leads to, where the decision model can
/// follow it (`None` once a modal opens or the press leaves the model).
fn follow(cx: EscapeContext, action: EscapeAction) -> Option<EscapeContext> {
    match action {
        A::GoTab(tab) => Some(EscapeContext { tab, ..cx }),
        A::Go(screen) => Some(EscapeContext { screen, ..cx }),
        _ => None,
    }
}

#[test]
fn properties() {
    let all = every_context();
    assert_eq!(all.len(), 7 * 9 * 12 * 256);
    for cx in all {
        let action = escape_action(&cx);
        // A focused text field and an open popup keep Escape for egui, and
        // nothing else does.
        assert_eq!(
            action == A::LeaveToEgui,
            matches!(cx.overlay, Overlay::TextFocus | Overlay::Popup),
            "{cx:?} -> {action:?}"
        );
        // The topmost overlay answers first: an overlay never navigates and
        // never reaches the Pause binding.
        if cx.overlay != Overlay::None {
            assert!(
                !action.navigates() && action != A::PassToPause,
                "{cx:?} -> {action:?}"
            );
        }
        // A pending durable transition is never raced by a navigation.
        if cx.durable_pending {
            assert!(!action.navigates(), "{cx:?} -> {action:?}");
        }
        // Escape reaches Pause only in a battle with nothing over the field,
        // construction closed and Pause bound to Escape (decision 18).
        assert_eq!(
            action == A::PassToPause,
            cx.screen == Screen::Battle
                && cx.overlay == Overlay::None
                && !cx.construction
                && cx.pause_is_escape,
            "{cx:?} -> {action:?}"
        );
        // Construction closes without pausing.
        if cx.screen == Screen::Battle && cx.overlay == Overlay::None && cx.construction {
            assert_eq!(action, A::CloseConstruction, "{cx:?}");
        }
        // The router consumes exactly what it acts on.
        assert_eq!(
            action.consumes(),
            !matches!(action, A::LeaveToEgui | A::PassToPause | A::Ignore),
            "{action:?}"
        );
        // Back only moves up the chain: to Camp home, Results or the menu.
        match action {
            A::GoTab(tab) => assert_eq!(tab, Tab::Home, "{cx:?}"),
            A::Go(screen) => {
                assert!(matches!(screen, Screen::Menu | Screen::Results), "{cx:?}");
                assert_ne!(screen, cx.screen, "{cx:?}");
            }
            _ => {}
        }
        // An unapplied difficulty draft matters only on Settings, where a
        // bare Escape opens the draft warning instead of leaving.
        if cx.screen != Screen::Settings {
            assert_eq!(
                action,
                escape_action(&EscapeContext {
                    draft_dirty: false,
                    ..cx
                }),
                "{cx:?}"
            );
        } else if cx.overlay == Overlay::None && !cx.durable_pending {
            assert_eq!(
                action,
                if cx.draft_dirty {
                    A::OpenModal(ModalKind::DraftWarning)
                } else {
                    A::SettingsBack
                },
                "{cx:?}"
            );
        }
        // The tab matters only on the camp.
        if cx.screen != Screen::Camp {
            assert_eq!(
                action,
                escape_action(&EscapeContext {
                    tab: Tab::Home,
                    ..cx
                }),
                "{cx:?}"
            );
        }
        // From any camp tab, Results or survivor selection, at most three
        // presses open the quit or return-to-menu confirmation or reach the
        // menu (design-ux-flow §10: <=5 presses to the Quit modal).
        if matches!(
            cx.screen,
            Screen::Camp | Screen::Results | Screen::SurvivorSelection
        ) && cx.overlay == Overlay::None
            && !cx.durable_pending
        {
            let mut here = cx;
            let mut reached = false;
            for _ in 0..3 {
                let step = escape_action(&here);
                if matches!(step, A::OpenModal(_)) || step == A::Go(Screen::Menu) {
                    reached = true;
                    break;
                }
                here = follow(here, step).unwrap_or_else(|| panic!("{here:?} -> {step:?}"));
            }
            assert!(reached, "{cx:?} never reaches a confirmation");
        }
    }
}
