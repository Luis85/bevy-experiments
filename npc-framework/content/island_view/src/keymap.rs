//! One table of the player's keys (design-ui section 5, rule 4): every key
//! that acts in the player session, per mode, with what it means and where
//! the player can learn it, a key cap on its control or a line of the
//! Guide's Controls page. Developer keys (`--dev-keys`) are not in it.
//!
//! The handlers that bind one key to one meaning look the key up here
//! ([`key`], the meanings in [`HANDLED`]); the host's Guide writes its
//! Controls page from [`guide_controls`]. Keys with modifiers, aliases
//! (the numpad) and the Shape tools still bind their own.
// `meaning` and some rows' fields are read by the tests only.
#![cfg_attr(not(test), allow(dead_code))]
use bevy::prelude::KeyCode;

/// Where a key acts. "Card" is a resident's card open in Oversee.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Mode {
    Live,
    Oversee,
    Card,
    Shape,
    Menus,
}

/// Modifier keys held with the key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Mods {
    None,
    Ctrl,
    Shift,
}

/// What a key does in one mode, and how the player learns it: the key cap
/// drawn on its control, or the Guide line of a key with no control.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Meaning {
    pub(crate) does: &'static str,
    pub(crate) cap: Option<&'static str>,
    pub(crate) guide: Option<&'static str>,
}

const fn cap(does: &'static str, cap: &'static str) -> Meaning {
    Meaning {
        does,
        cap: Some(cap),
        guide: None,
    }
}

const fn guide(does: &'static str, line: &'static str) -> Meaning {
    Meaning {
        does,
        cap: None,
        guide: Some(line),
    }
}

use KeyCode as K;
use Mode::{Card, Live, Menus, Oversee, Shape};
use Mods::{Ctrl, Shift};

/// Every player key, by mode. One row per (mode, key, modifiers).
pub(crate) const KEYS: &[(Mode, KeyCode, Mods, Meaning)] = &[
    // Every mode of play: time, modes, saving.
    (
        Live,
        K::Space,
        Mods::None,
        cap("pause or run time", "Space"),
    ),
    (
        Oversee,
        K::Space,
        Mods::None,
        cap("pause or run time", "Space"),
    ),
    (
        Card,
        K::Space,
        Mods::None,
        cap("pause or run time", "Space"),
    ),
    (
        Shape,
        K::Space,
        Mods::None,
        cap("pause or run time", "Space"),
    ),
    (Live, K::Minus, Mods::None, cap("slower", "-")),
    (Oversee, K::Minus, Mods::None, cap("slower", "-")),
    (Card, K::Minus, Mods::None, cap("slower", "-")),
    (Shape, K::Minus, Mods::None, cap("slower", "-")),
    (Live, K::Equal, Mods::None, cap("faster", "=")),
    (Oversee, K::Equal, Mods::None, cap("faster", "=")),
    (Card, K::Equal, Mods::None, cap("faster", "=")),
    (Shape, K::Equal, Mods::None, cap("faster", "=")),
    (Live, K::Digit1, Mods::None, cap("Live", "1")),
    (Oversee, K::Digit1, Mods::None, cap("Live", "1")),
    (Card, K::Digit1, Mods::None, cap("Live", "1")),
    (Shape, K::Digit1, Mods::None, cap("Live", "1")),
    (Live, K::Digit2, Mods::None, cap("Oversee", "2")),
    (Oversee, K::Digit2, Mods::None, cap("Oversee", "2")),
    (Card, K::Digit2, Mods::None, cap("Oversee", "2")),
    (Shape, K::Digit2, Mods::None, cap("Oversee", "2")),
    (Live, K::Digit3, Mods::None, cap("Shape", "3")),
    (Oversee, K::Digit3, Mods::None, cap("Shape", "3")),
    (Card, K::Digit3, Mods::None, cap("Shape", "3")),
    (Shape, K::Digit3, Mods::None, cap("Shape", "3")),
    (Live, K::KeyS, Ctrl, cap("save", "Ctrl+S")),
    (Oversee, K::KeyS, Ctrl, cap("save", "Ctrl+S")),
    (Card, K::KeyS, Ctrl, cap("save", "Ctrl+S")),
    (Shape, K::KeyS, Ctrl, cap("save", "Ctrl+S")),
    (Menus, K::KeyS, Ctrl, cap("Save game", "Ctrl+S")),
    // Esc: the ladder of each mode.
    (
        Live,
        K::Escape,
        Mods::None,
        cap("close the open tab, else the pause menu", "Esc"),
    ),
    (
        Oversee,
        K::Escape,
        Mods::None,
        cap("close what is open, else the pause menu", "Esc"),
    ),
    (Card, K::Escape, Mods::None, cap("close the card", "Esc")),
    (
        Shape,
        K::Escape,
        Mods::None,
        cap("put down the item or leave the tool", "Esc"),
    ),
    (Menus, K::Escape, Mods::None, cap("back", "Esc")),
    // Keyboard focus.
    (
        Live,
        K::Tab,
        Mods::None,
        guide(
            "walk the controls; with no tab open, open the Pouch",
            "Tab walks the controls.",
        ),
    ),
    (
        Oversee,
        K::Tab,
        Mods::None,
        guide("walk the controls", "Tab walks the controls."),
    ),
    (
        Card,
        K::Tab,
        Mods::None,
        guide("walk the card's controls", "Tab walks the controls."),
    ),
    (
        Shape,
        K::Tab,
        Mods::None,
        guide("walk the controls", "Tab walks the controls."),
    ),
    (
        Menus,
        K::Tab,
        Mods::None,
        guide("walk the controls", "Tab walks the controls."),
    ),
    (
        Live,
        K::Tab,
        Shift,
        guide("walk back", "Shift+Tab walks back."),
    ),
    (
        Oversee,
        K::Tab,
        Shift,
        guide("walk back", "Shift+Tab walks back."),
    ),
    (
        Card,
        K::Tab,
        Shift,
        guide("walk back", "Shift+Tab walks back."),
    ),
    (
        Shape,
        K::Tab,
        Shift,
        guide("walk back", "Shift+Tab walks back."),
    ),
    (
        Menus,
        K::Tab,
        Shift,
        guide("walk back", "Shift+Tab walks back."),
    ),
    (
        Live,
        K::Enter,
        Mods::None,
        guide(
            "press the focused control",
            "Enter presses the focused control.",
        ),
    ),
    (
        Oversee,
        K::Enter,
        Mods::None,
        guide(
            "press the focused control",
            "Enter presses the focused control.",
        ),
    ),
    (
        Card,
        K::Enter,
        Mods::None,
        guide(
            "press the focused control",
            "Enter presses the focused control.",
        ),
    ),
    (
        Shape,
        K::Enter,
        Mods::None,
        guide(
            "press the focused control",
            "Enter presses the focused control.",
        ),
    ),
    (
        Menus,
        K::Enter,
        Mods::None,
        guide(
            "press the focused control",
            "Enter presses the focused control.",
        ),
    ),
    (
        Live,
        K::AltLeft,
        Mods::None,
        guide("longer tooltip", "Hold Alt for a longer tooltip."),
    ),
    (
        Oversee,
        K::AltLeft,
        Mods::None,
        guide("longer tooltip", "Hold Alt for a longer tooltip."),
    ),
    (
        Card,
        K::AltLeft,
        Mods::None,
        guide("longer tooltip", "Hold Alt for a longer tooltip."),
    ),
    (
        Shape,
        K::AltLeft,
        Mods::None,
        guide("longer tooltip", "Hold Alt for a longer tooltip."),
    ),
    (
        Menus,
        K::AltLeft,
        Mods::None,
        guide("longer tooltip", "Hold Alt for a longer tooltip."),
    ),
    // Moving and the camera.
    (
        Live,
        K::KeyW,
        Mods::None,
        guide("walk", "W A S D walk in Live."),
    ),
    (
        Live,
        K::KeyA,
        Mods::None,
        guide("walk", "W A S D walk in Live."),
    ),
    (
        Live,
        K::KeyS,
        Mods::None,
        guide("walk", "W A S D walk in Live."),
    ),
    (
        Live,
        K::KeyD,
        Mods::None,
        guide("walk", "W A S D walk in Live."),
    ),
    (
        Oversee,
        K::KeyW,
        Mods::None,
        guide("pan the camera", "W A S D and the arrows pan the camera."),
    ),
    (
        Oversee,
        K::KeyA,
        Mods::None,
        guide("pan the camera", "W A S D and the arrows pan the camera."),
    ),
    (
        Oversee,
        K::KeyS,
        Mods::None,
        guide("pan the camera", "W A S D and the arrows pan the camera."),
    ),
    (
        Oversee,
        K::KeyD,
        Mods::None,
        guide("pan the camera", "W A S D and the arrows pan the camera."),
    ),
    (
        Card,
        K::KeyW,
        Mods::None,
        guide("pan the camera", "W A S D and the arrows pan the camera."),
    ),
    (
        Card,
        K::KeyA,
        Mods::None,
        guide("pan the camera", "W A S D and the arrows pan the camera."),
    ),
    (
        Card,
        K::KeyS,
        Mods::None,
        guide("pan the camera", "W A S D and the arrows pan the camera."),
    ),
    (
        Card,
        K::KeyD,
        Mods::None,
        guide("pan the camera", "W A S D and the arrows pan the camera."),
    ),
    (
        Shape,
        K::KeyW,
        Mods::None,
        guide("pan the camera", "W A S D and the arrows pan the camera."),
    ),
    (
        Shape,
        K::KeyA,
        Mods::None,
        guide("pan the camera", "W A S D and the arrows pan the camera."),
    ),
    (
        Shape,
        K::KeyS,
        Mods::None,
        guide("pan the camera", "W A S D and the arrows pan the camera."),
    ),
    (
        Shape,
        K::KeyD,
        Mods::None,
        guide("pan the camera", "W A S D and the arrows pan the camera."),
    ),
    (
        Oversee,
        K::ArrowUp,
        Mods::None,
        guide("pan the camera", "W A S D and the arrows pan the camera."),
    ),
    (
        Oversee,
        K::ArrowDown,
        Mods::None,
        guide("pan the camera", "W A S D and the arrows pan the camera."),
    ),
    (
        Oversee,
        K::ArrowLeft,
        Mods::None,
        guide("pan the camera", "W A S D and the arrows pan the camera."),
    ),
    (
        Oversee,
        K::ArrowRight,
        Mods::None,
        guide("pan the camera", "W A S D and the arrows pan the camera."),
    ),
    (
        Shape,
        K::ArrowUp,
        Mods::None,
        guide("pan the camera", "W A S D and the arrows pan the camera."),
    ),
    (
        Shape,
        K::ArrowDown,
        Mods::None,
        guide("pan the camera", "W A S D and the arrows pan the camera."),
    ),
    (
        Shape,
        K::ArrowLeft,
        Mods::None,
        guide("pan the camera", "W A S D and the arrows pan the camera."),
    ),
    (
        Shape,
        K::ArrowRight,
        Mods::None,
        guide("pan the camera", "W A S D and the arrows pan the camera."),
    ),
    (
        Live,
        K::KeyQ,
        Mods::None,
        guide("turn the camera left", "Q and R turn the camera in Live."),
    ),
    (
        Live,
        K::KeyR,
        Mods::None,
        guide("turn the camera right", "Q and R turn the camera in Live."),
    ),
    (
        Oversee,
        K::KeyQ,
        Mods::None,
        guide("turn the camera left", "Q and E turn the camera."),
    ),
    (
        Oversee,
        K::KeyE,
        Mods::None,
        guide("turn the camera right", "Q and E turn the camera."),
    ),
    (
        Card,
        K::KeyQ,
        Mods::None,
        guide("turn the camera left", "Q and E turn the camera."),
    ),
    (
        Card,
        K::KeyE,
        Mods::None,
        guide("turn the camera right", "Q and E turn the camera."),
    ),
    (
        Shape,
        K::KeyQ,
        Mods::None,
        guide("turn the camera left", "Q and E turn the camera."),
    ),
    (
        Shape,
        K::KeyE,
        Mods::None,
        guide("turn the camera right", "Q and E turn the camera."),
    ),
    (
        Oversee,
        K::KeyR,
        Mods::None,
        cap("turn the building in hand", "R"),
    ),
    (
        Oversee,
        K::PageUp,
        Mods::None,
        guide(
            "scroll the ledger, else move the camera",
            "PageUp and PageDown scroll the open page.",
        ),
    ),
    (
        Oversee,
        K::PageDown,
        Mods::None,
        guide(
            "scroll the ledger, else move the camera",
            "PageUp and PageDown scroll the open page.",
        ),
    ),
    (
        Card,
        K::PageUp,
        Mods::None,
        guide(
            "scroll the card",
            "PageUp and PageDown scroll the open page.",
        ),
    ),
    (
        Card,
        K::PageDown,
        Mods::None,
        guide(
            "scroll the card",
            "PageUp and PageDown scroll the open page.",
        ),
    ),
    (
        Shape,
        K::PageUp,
        Mods::None,
        guide(
            "move the camera",
            "PageUp and PageDown move the camera in Shape.",
        ),
    ),
    (
        Shape,
        K::PageDown,
        Mods::None,
        guide(
            "move the camera",
            "PageUp and PageDown move the camera in Shape.",
        ),
    ),
    (
        Menus,
        K::PageUp,
        Mods::None,
        guide("scroll", "PageUp and PageDown scroll the open page."),
    ),
    (
        Menus,
        K::PageDown,
        Mods::None,
        guide("scroll", "PageUp and PageDown scroll the open page."),
    ),
    // Live: what is in reach and the tabs of the hotbar.
    (Live, K::KeyE, Mods::None, cap("use what is in reach", "E")),
    (
        Live,
        K::KeyX,
        Mods::None,
        cap("stop walking or working", "X"),
    ),
    (Live, K::KeyI, Mods::None, cap("Pouch", "I")),
    (Live, K::KeyC, Mods::None, cap("Craft", "C")),
    (Live, K::KeyB, Mods::None, cap("Build", "B")),
    (Live, K::KeyM, Mods::None, cap("Explore", "M")),
    (Live, K::KeyT, Mods::None, cap("Trade", "T")),
    (Live, K::KeyG, Mods::None, cap("Garden", "G")),
    (Live, K::KeyN, Mods::None, cap("People", "N")),
    (
        Live,
        K::KeyL,
        Mods::None,
        cap("choose a successor after a death", "L"),
    ),
    // Oversee: the build tray, places, overlays and the ledger pages.
    (Oversee, K::KeyB, Mods::None, cap("build tray", "B")),
    (Oversee, K::KeyM, Mods::None, cap("places", "M")),
    (Oversee, K::KeyO, Mods::None, cap("overlays", "O")),
    (Card, K::KeyB, Mods::None, cap("build tray", "B")),
    (Card, K::KeyM, Mods::None, cap("places", "M")),
    (Card, K::KeyO, Mods::None, cap("overlays", "O")),
    (Card, K::KeyF, Mods::None, cap("follow this resident", "F")),
    (Oversee, K::F1, Mods::None, cap("ledger: Village", "F1")),
    (Oversee, K::F2, Mods::None, cap("ledger: Economy", "F2")),
    (Oversee, K::F3, Mods::None, cap("ledger: People", "F3")),
    (Oversee, K::F4, Mods::None, cap("ledger: Projects", "F4")),
    (Oversee, K::F5, Mods::None, cap("ledger: Journal", "F5")),
    (Card, K::F1, Mods::None, cap("ledger: Village", "F1")),
    (Card, K::F2, Mods::None, cap("ledger: Economy", "F2")),
    (Card, K::F3, Mods::None, cap("ledger: People", "F3")),
    (Card, K::F4, Mods::None, cap("ledger: Projects", "F4")),
    (Card, K::F5, Mods::None, cap("ledger: Journal", "F5")),
    // Shape: the tools, the brush and the timeline.
    (
        Shape,
        K::KeyR,
        Mods::None,
        cap("Raise; with a plot or prop in hand, turn it", "R"),
    ),
    (Shape, K::KeyF, Mods::None, cap("Lower", "F")),
    (Shape, K::KeyT, Mods::None, cap("Flatten", "T")),
    (Shape, K::KeyG, Mods::None, cap("Smooth", "G")),
    (Shape, K::KeyP, Mods::None, cap("Paint", "P")),
    (Shape, K::KeyV, Mods::None, cap("next surface", "V")),
    (Shape, K::KeyH, Mods::None, cap("hard or soft edge", "H")),
    (Shape, K::BracketLeft, Mods::None, cap("brush smaller", "[")),
    (Shape, K::BracketRight, Mods::None, cap("brush larger", "]")),
    (Shape, K::KeyZ, Ctrl, cap("undo", "Ctrl+Z")),
    (Shape, K::KeyY, Ctrl, cap("redo", "Ctrl+Y")),
    (Shape, K::KeyN, Mods::None, cap("place a patch", "N")),
    (Shape, K::KeyX, Mods::None, cap("remove a patch", "X")),
    (
        Shape,
        K::KeyL,
        Mods::None,
        cap("place plots and props", "L"),
    ),
    // Menus: Space presses the focused control there.
    (
        Menus,
        K::Space,
        Mods::None,
        guide(
            "press the focused control",
            "Space presses the focused control in menus.",
        ),
    ),
];

/// What `key` with `mods` means in `mode`, if anything.
pub(crate) fn meaning(mode: Mode, key: KeyCode, mods: Mods) -> Option<Meaning> {
    KEYS.iter()
        .find(|(at, bound, held, _)| *at == mode && *bound == key && *held == mods)
        .map(|(_, _, _, meaning)| *meaning)
}

/// The key bound to `does` in `mode`, for a handler that binds one key to
/// one meaning: the table is the one source. A meaning not in the table is
/// a programming error, caught by `every_key_a_handler_asks_for_is_in_the_table`.
pub(crate) fn key(mode: Mode, does: &str) -> KeyCode {
    KEYS.iter()
        .find(|(at, _, mods, meaning)| *at == mode && *mods == Mods::None && meaning.does == does)
        .map(|(_, key, _, _)| *key)
        .unwrap_or_else(|| panic!("no key for {does:?} in {mode:?}"))
}

/// What a handler asks of the table: `(mode, meaning)`. The test below
/// looks every one of them up.
pub(crate) const HANDLED: &[(Mode, &str)] = &[
    (Live, LIVE),
    (Live, OVERSEE),
    (Live, SHAPE),
    (Live, PAUSE),
    (Live, SLOWER),
    (Live, FASTER),
    (Live, POUCH),
    (Live, CRAFT),
    (Live, BUILD),
    (Live, GARDEN),
    (Live, TRADE),
    (Live, PEOPLE),
    (Live, EXPLORE),
    (Live, USE),
    (Live, STOP),
    (Live, SUCCESSOR),
    (Oversee, BUILD_TRAY),
    (Oversee, OVERLAYS),
    (Oversee, PLACES),
    (Oversee, LEDGER_VILLAGE),
    (Oversee, LEDGER_ECONOMY),
    (Oversee, LEDGER_PEOPLE),
    (Oversee, LEDGER_PROJECTS),
    (Oversee, LEDGER_JOURNAL),
    (Card, FOLLOW),
];

/// The meanings the handlers look up, as the table words them.
pub(crate) const LIVE: &str = "Live";
pub(crate) const OVERSEE: &str = "Oversee";
pub(crate) const SHAPE: &str = "Shape";
pub(crate) const PAUSE: &str = "pause or run time";
pub(crate) const SLOWER: &str = "slower";
pub(crate) const FASTER: &str = "faster";
pub(crate) const POUCH: &str = "Pouch";
pub(crate) const CRAFT: &str = "Craft";
pub(crate) const BUILD: &str = "Build";
pub(crate) const GARDEN: &str = "Garden";
pub(crate) const TRADE: &str = "Trade";
pub(crate) const PEOPLE: &str = "People";
pub(crate) const EXPLORE: &str = "Explore";
pub(crate) const USE: &str = "use what is in reach";
pub(crate) const STOP: &str = "stop walking or working";
pub(crate) const SUCCESSOR: &str = "choose a successor after a death";
pub(crate) const BUILD_TRAY: &str = "build tray";
pub(crate) const OVERLAYS: &str = "overlays";
pub(crate) const PLACES: &str = "places";
pub(crate) const LEDGER_VILLAGE: &str = "ledger: Village";
pub(crate) const LEDGER_ECONOMY: &str = "ledger: Economy";
pub(crate) const LEDGER_PEOPLE: &str = "ledger: People";
pub(crate) const LEDGER_PROJECTS: &str = "ledger: Projects";
pub(crate) const LEDGER_JOURNAL: &str = "ledger: Journal";
pub(crate) const FOLLOW: &str = "follow this resident";

/// One section of the Guide's Controls page: a mode and its keys, each
/// line `(keys, what they do)`, in table order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuideSection {
    /// "Live", "Oversee", "A resident's card", "Shape", "Menus".
    pub mode: &'static str,
    /// `("W A S D", "walk")`, `("Ctrl+S", "save")`: the keys that share a
    /// meaning stand on one line.
    pub keys: Vec<(String, String)>,
}

/// A key as the Guide writes it: "W", "1", "Esc", "Shift+Tab", "[".
fn key_name(key: KeyCode, mods: Mods) -> String {
    let name = match key {
        K::Space => "Space".to_owned(),
        K::Escape => "Esc".to_owned(),
        K::Tab => "Tab".to_owned(),
        K::Enter => "Enter".to_owned(),
        K::AltLeft => "Alt".to_owned(),
        K::Minus => "-".to_owned(),
        K::Equal => "=".to_owned(),
        K::BracketLeft => "[".to_owned(),
        K::BracketRight => "]".to_owned(),
        K::ArrowUp => "Up".to_owned(),
        K::ArrowDown => "Down".to_owned(),
        K::ArrowLeft => "Left".to_owned(),
        K::ArrowRight => "Right".to_owned(),
        K::PageUp => "PageUp".to_owned(),
        K::PageDown => "PageDown".to_owned(),
        other => {
            let debug = format!("{other:?}");
            debug
                .strip_prefix("Key")
                .or_else(|| debug.strip_prefix("Digit"))
                .unwrap_or(&debug)
                .to_owned()
        }
    };
    match mods {
        Mods::None => name,
        Mods::Ctrl => format!("Ctrl+{name}"),
        Mods::Shift => format!("Shift+{name}"),
    }
}

fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Live => "Live",
        Oversee => "Oversee",
        Card => "A resident's card",
        Shape => "Shape",
        Menus => "Menus",
    }
}

/// The Guide's Controls page: every player key per mode, the keys of one
/// meaning on one line. Developer keys are not in the table, so not here.
pub fn guide_controls() -> Vec<GuideSection> {
    // A key bound alike in every mode of play is said once, first.
    let plays = [Live, Oversee, Card, Shape];
    let everywhere = |key: KeyCode, mods: Mods, does: &str| {
        plays.iter().all(|mode| {
            KEYS.iter().any(|(at, bound, held, meaning)| {
                at == mode && *bound == key && *held == mods && meaning.does == does
            })
        })
    };
    let mut sections: Vec<GuideSection> = vec![GuideSection {
        mode: "Every mode",
        keys: Vec::new(),
    }];
    for (mode, key, mods, meaning) in KEYS {
        let shared = *mode != Menus && everywhere(*key, *mods, meaning.does);
        let name = if shared {
            "Every mode"
        } else {
            mode_name(*mode)
        };
        if !sections.iter().any(|section| section.mode == name) {
            sections.push(GuideSection {
                mode: name,
                keys: Vec::new(),
            });
        }
        let section = sections
            .iter_mut()
            .find(|section| section.mode == name)
            .expect("its section");
        let label = key_name(*key, *mods);
        let does = crate::game_ui::words::capital(meaning.does);
        match section.keys.iter_mut().find(|(_, said)| *said == does) {
            // The same key again (it was said for another mode already).
            Some((keys, _)) if keys.split(' ').any(|held| held == label) => {}
            Some((keys, _)) => {
                keys.push(' ');
                keys.push_str(&label);
            }
            None => section.keys.push((label, does)),
        }
    }
    let order = [
        "Every mode",
        "Live",
        "Oversee",
        "A resident's card",
        "Shape",
        "Menus",
    ];
    sections.retain(|section| !section.keys.is_empty());
    sections.sort_by_key(|section| order.iter().position(|name| *name == section.mode));
    sections
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_two_meanings_share_a_key_in_one_mode() {
        for (index, (mode, key, mods, meaning)) in KEYS.iter().enumerate() {
            for (other_mode, other_key, other_mods, other) in &KEYS[index + 1..] {
                assert!(
                    !(mode == other_mode && key == other_key && mods == other_mods),
                    "{mode:?} {key:?} {mods:?} means both {:?} and {:?}",
                    meaning.does,
                    other.does
                );
            }
        }
        assert_eq!(
            meaning(Shape, K::KeyE, Mods::None).map(|meaning| meaning.does),
            Some("turn the camera right"),
            "E is the camera in Shape; Remove is X"
        );
        assert_eq!(
            meaning(Live, K::KeyE, Mods::None).map(|meaning| meaning.does),
            Some("use what is in reach")
        );
    }

    #[test]
    fn every_key_a_handler_asks_for_is_in_the_table() {
        for (mode, does) in HANDLED {
            let bound = key(*mode, does);
            assert_eq!(
                meaning(*mode, bound, Mods::None).map(|m| m.does),
                Some(*does)
            );
        }
        assert_eq!(key(Live, POUCH), K::KeyI);
        assert_eq!(key(Oversee, LEDGER_ECONOMY), K::F2);
    }

    #[test]
    fn the_guide_lists_each_modes_keys_on_shared_lines() {
        let guide = guide_controls();
        let modes: Vec<&str> = guide.iter().map(|section| section.mode).collect();
        assert_eq!(
            modes,
            [
                "Every mode",
                "Live",
                "Oversee",
                "A resident's card",
                "Shape",
                "Menus"
            ]
        );
        let has = |section: usize, keys: &str, does: &str| {
            guide[section]
                .keys
                .contains(&(keys.to_owned(), does.to_owned()))
        };
        // Said once, first: the keys bound alike in every mode of play.
        assert!(has(0, "Ctrl+S", "Save"), "{:?}", guide[0].keys);
        assert!(has(0, "Space", "Pause or run time"));
        assert!(guide[1].keys.iter().all(|(keys, _)| keys != "Ctrl+S"));
        // Each mode's own, capitalised.
        assert!(has(1, "W A S D", "Walk"), "{:?}", guide[1].keys);
        assert!(has(1, "I", "Pouch"));
        assert!(has(4, "Ctrl+Z", "Undo"));
        assert!(has(4, "[", "Brush smaller"));
        // No section has two lines of one meaning.
        for section in &guide {
            for (index, (_, does)) in section.keys.iter().enumerate() {
                assert!(
                    section.keys[index + 1..]
                        .iter()
                        .all(|(_, other)| other != does),
                    "{} lists {does:?} twice",
                    section.mode
                );
            }
        }
    }

    #[test]
    fn every_player_key_has_a_key_cap_or_a_guide_line() {
        for (mode, key, mods, meaning) in KEYS {
            assert!(
                meaning.cap.is_some_and(|cap| !cap.is_empty())
                    || meaning.guide.is_some_and(|line| !line.is_empty()),
                "{mode:?} {key:?} {mods:?} ({}) is a hidden key",
                meaning.does
            );
            assert!(!meaning.does.is_empty());
        }
        // The retired keys are not player keys any more.
        for retired in [K::F11, K::F9, K::Period] {
            assert!(
                KEYS.iter().all(|(_, key, _, _)| *key != retired),
                "{retired:?} is a developer key now"
            );
        }
        assert!(
            KEYS.iter()
                .all(|(mode, key, _, _)| !(*mode == Live && *key == K::F2)),
            "F2 does not open the workspace in Live"
        );
    }
}
