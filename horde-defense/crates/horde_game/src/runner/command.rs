//! Typed queue entries and frontend-facing runner messages.
use bevy::prelude::*;
use horde_core::{
    command::{Command, CommandOutcome},
    sim::{Input, SettlementIssue},
};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveIntent {
    Manual,
    Menu,
    Quit,
    SettingsBack,
    Autosave,
}
/// Queue entry published by the frontend: gameplay commands applied through
/// `Game::apply`, plus the durable reset and save intents the runner owns.
#[derive(Clone, Debug, PartialEq)]
pub enum AppCommand {
    /// Serialized storage barrier; never reaches `Game::apply`.
    NewJourney,
    Save(SaveIntent),
    /// Shows the profile directory in the OS file manager through the
    /// runtime's `FolderOpener`; answered by `RuntimeNotice::SaveFolderOpened`.
    OpenSaveFolder,
    /// Reads `campaign.json` again after a failed startup load; answered by
    /// `RuntimeNotice::ProfileReloaded`. Refused once the in-memory journey
    /// or the file changed (`Runtime::reload_allowed`).
    ReloadProfile,
    Game(Command),
}
/// The error of a save a confirmed new journey replaced: not a storage
/// failure, so the frontend offers no retry for it.
pub const SAVE_SUPERSEDED: &str = "Save superseded by the confirmed new journey";
impl From<Command> for AppCommand {
    fn from(command: Command) -> Self {
        Self::Game(command)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppOutcome {
    Command(CommandOutcome),
    JourneyArchived(Option<PathBuf>),
}
/// Outcomes the frontend reacts to; the UI's notice system turns them into
/// feedback text and screen transitions.
#[derive(Message, Clone, Debug)]
pub enum RuntimeNotice {
    CommandCompleted {
        command: AppCommand,
        result: Result<AppOutcome, String>,
        success: String,
    },
    Saved {
        intent: SaveIntent,
        result: Result<(), String>,
    },
    /// A user intent finished without a save (`Runtime::forbid_saves`): its
    /// navigation still runs, but nothing was written.
    SaveSkipped {
        intent: SaveIntent,
    },
    /// The campaign was saved but settings.json was not
    /// (`persistence::Commit::SavedWithoutSettings`); a later save retries it.
    SettingsNotSaved(String),
    SimulationFinished,
    /// `AppCommand::OpenSaveFolder` finished: the file manager was started,
    /// or why it could not be.
    SaveFolderOpened(Result<(), String>),
    /// `AppCommand::ReloadProfile` finished: the saved journey replaced the
    /// in-memory one, or why it did not.
    ProfileReloaded(Result<(), String>),
    RecoveryCopies(Vec<PathBuf>),
    /// Expedition transfers that could not be applied to the shared campaign.
    SettlementIssues(Vec<SettlementIssue>),
}
#[derive(Clone, Copy, Debug, Default)]
pub struct AttackAims {
    pub fire: Option<[f32; 2]>,
    pub melee: Option<[f32; 2]>,
}
#[derive(Clone, Debug)]
pub struct ControlFrame {
    pub enabled: bool,
    pub speed: u32,
    pub held: Input,
    pub pressed: Input,
    pub pressed_aims: AttackAims,
}
impl Default for ControlFrame {
    fn default() -> Self {
        Self {
            enabled: false,
            speed: 1,
            held: Input::default(),
            pressed: Input::default(),
            pressed_aims: AttackAims::default(),
        }
    }
}
