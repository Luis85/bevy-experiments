//! Frontend Bevy states. `Screen` is the top-level view; `Tab` is the camp tab,
//! a sub-state that exists only while the camp is shown. Per-screen policy
//! (header chrome, settings return label, ambience) lives here as exhaustive
//! matches so a new screen cannot be forgotten.
use bevy::prelude::*;

#[derive(States, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Screen {
    #[default]
    Menu,
    SurvivorSelection,
    Camp,
    Battle,
    /// The debrief of the settled round: outcome, loot and three actions.
    Results,
    /// The upgrade tree, with the way back to the battle and to the camp.
    Upgrades,
    History,
    Settings,
}

/// How a screen colours the ambience bed, before battle state is considered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenAmbience {
    Menu,
    Paused,
    /// Follows the live battle or camp.
    Live,
}

impl Screen {
    pub const ALL: [Screen; 8] = [
        Screen::Menu,
        Screen::SurvivorSelection,
        Screen::Camp,
        Screen::Battle,
        Screen::Results,
        Screen::Upgrades,
        Screen::History,
        Screen::Settings,
    ];
    /// Whether the persistent LASTLIGHT header (wallet, Menu, History,
    /// Settings) is drawn. Camp, Results and Upgrades share it.
    pub fn shows_header(self) -> bool {
        match self {
            Screen::Menu | Screen::SurvivorSelection | Screen::Battle => false,
            Screen::Camp
            | Screen::Results
            | Screen::Upgrades
            | Screen::History
            | Screen::Settings => true,
        }
    }
    /// Label of the settings screen's save-and-return action.
    pub fn settings_return_label(self) -> &'static str {
        match self {
            Screen::Menu | Screen::Settings => "Save and return to main menu",
            Screen::SurvivorSelection => "Save and return to survivor selection",
            Screen::Camp => "Save and return to camp",
            Screen::Battle => "Save and return to defense",
            Screen::Results => "Save and return to results",
            Screen::Upgrades => "Save and return to upgrades",
            Screen::History => "Save and return to run history",
        }
    }
    /// What the music director scores on this screen and camp tab. The
    /// briefing is its own entry (scored as the camp); Settings and History
    /// lie over what was playing.
    pub fn music(self, tab: Tab) -> horde_audio::music::MusicScreen {
        use horde_audio::music::MusicScreen;
        match self {
            Screen::Menu | Screen::SurvivorSelection => MusicScreen::Menu,
            Screen::Camp if tab == Tab::Briefing => MusicScreen::Briefing,
            Screen::Camp => MusicScreen::Camp,
            Screen::Battle => MusicScreen::Battle,
            Screen::Results => MusicScreen::Results,
            Screen::Upgrades => MusicScreen::Upgrades,
            Screen::History | Screen::Settings => MusicScreen::Overlay,
        }
    }
    pub fn ambience(self) -> ScreenAmbience {
        match self {
            Screen::Menu | Screen::SurvivorSelection => ScreenAmbience::Menu,
            Screen::Settings | Screen::Results | Screen::Upgrades => ScreenAmbience::Paused,
            Screen::Camp | Screen::Battle | Screen::History => ScreenAmbience::Live,
        }
    }
}

#[derive(SubStates, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[source(Screen = Screen::Camp)]
pub enum Tab {
    /// Camp home: the next defense, the roster, alerts and the world clock
    /// (design-ux-flow decision 5). The default camp tab.
    #[default]
    Home,
    World,
    Briefing,
    Character,
    /// Mara's training: a drill-down from Camp home, not a row entry.
    Survivor,
    Armory,
    Skills,
    Engineering,
    /// Expeditions, outbreaks and roster orders: a drill-down from Camp home.
    Operations,
}
impl Tab {
    /// Whether the tab is a drill-down under Camp home: it renders the
    /// "‹ Camp home" breadcrumb and the row keeps Camp selected.
    pub fn is_home_drill_down(self) -> bool {
        matches!(self, Tab::Survivor | Tab::Operations)
    }
    /// The drill-down's name in its breadcrumb.
    pub fn drill_down_name(self) -> Option<&'static str> {
        match self {
            Tab::Survivor => Some(horde_core::skill_catalog::CampaignFeature::Companion.label()),
            Tab::Operations => Some("Operations"),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_screen_is_listed_once_and_returns_somewhere() {
        for (index, screen) in Screen::ALL.iter().enumerate() {
            assert_eq!(Screen::ALL.iter().position(|s| s == screen), Some(index));
            assert!(
                screen
                    .settings_return_label()
                    .starts_with("Save and return to")
            );
        }
        assert!(Screen::History.shows_header());
        assert!(Screen::Camp.shows_header());
        assert!(Screen::Results.shows_header());
        assert!(Screen::Upgrades.shows_header());
        assert_eq!(Screen::ALL.len(), 8);
        assert_eq!(Screen::Upgrades.ambience(), ScreenAmbience::Paused);
        assert_eq!(
            Screen::Upgrades.settings_return_label(),
            "Save and return to upgrades"
        );
        assert_eq!(
            Screen::Results.settings_return_label(),
            "Save and return to results"
        );
        assert!(!Screen::Battle.shows_header());
    }
}
