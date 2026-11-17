//! W3-06 (UX-08): the storage-error modal (design-ux-flow §4 "Storage
//! modal"). A failed save, an unreadable save and unreadable settings each
//! open it, with the ways on the design names; "Open folder" and "Retry
//! load" go through the runner.
use super::*;
use crate::{
    runner::{RecordingOpener, RuntimeNotice},
    ui::modal::{
        AppModal, CONTINUE_FRESH, COPY_PATH, DISMISS, LOAD_FAILED_TITLE, OPEN_FOLDER, RETRY,
        RETRY_LOAD, SAVE_FAILED_TITLE, SETTINGS_FAILED_TITLE, StorageError, USE_DEFAULTS,
    },
};

/// The labels of the last frame's modal primaries.
fn modal_primaries(h: &Harness) -> Vec<String> {
    h.primaries()
        .into_iter()
        .filter(|record| record.surface == Surface::Modal)
        .map(|record| record.label)
        .collect()
}

/// The frame that shows a modal which just opened: egui sizes a new modal in
/// an invisible first pass, so the second frame is the one that paints it.
fn shown(h: &Harness, f: &mut FrontendParts) -> FullOutput {
    h.frame(f, View::Full, vec![]);
    h.frame(f, View::Full, vec![])
}

/// The runner's answer to a save of `intent` that failed with `error`.
fn save_failed(f: &mut FrontendParts, intent: SaveIntent, error: &str) {
    f.notices.push(RuntimeNotice::Saved {
        intent,
        result: Err(error.into()),
    });
    f.apply_notices();
}

/// The open save-failure modal's intent and error.
fn save_modal(f: &FrontendParts) -> Option<(SaveIntent, String)> {
    match &f.status.modal {
        Some(AppModal::Storage(StorageError::Save { intent, error })) => {
            Some((*intent, error.clone()))
        }
        _ => None,
    }
}

/// The frontend as startup builds it from the files already in `directory`.
fn frontend_loaded_from(directory: &std::path::Path) -> FrontendParts {
    let paths = ProfilePaths::in_directory(directory.to_path_buf());
    let startup = crate::profile::read_settings(&paths.settings());
    let profile = crate::profile::LoadedProfile::load(&paths, startup);
    FrontendParts::new(profile, &crate::launch::LaunchConfig::default(), &paths)
}

/// Ends `f`'s storage worker and removes the campaign files and recovery
/// copies it left in `directory` (the fixture removes the rest).
fn finish(mut f: FrontendParts, directory: &std::path::Path) {
    f.runtime.finish_storage_for_test();
    for entry in std::fs::read_dir(directory).into_iter().flatten().flatten() {
        if entry.file_name().to_string_lossy().starts_with("campaign.") {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[test]
fn save_failure_opens_retry_modal() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let directory = fixture.directory.clone();
    let f = &mut fixture.frontend;
    let opener = RecordingOpener::default();
    f.runtime.set_folder_opener(opener.clone());
    // A file cannot be used as a parent directory, on any supported OS.
    std::fs::create_dir_all(&directory).unwrap();
    let blocker = directory.join("blocked");
    std::fs::write(&blocker, b"blocked").unwrap();
    let blocked = ProfilePaths::files(blocker.join("campaign.json"), f.paths.settings());
    let open = std::mem::replace(&mut f.paths, blocked);
    // The storage error takes the single modal slot from an open modal.
    f.status.modal = Some(AppModal::Quit);
    f.ctx().request_save(SaveIntent::Manual);
    f.flush_for_test();
    let (intent, error) = save_modal(f).expect("a failed save opens the storage modal");
    assert_eq!(intent, SaveIntent::Manual);
    assert!(!error.is_empty());
    assert!(!f.status.save_pending);
    let output = shown(&h, f);
    assert_eq!(text_count(&output, SAVE_FAILED_TITLE), 1);
    assert_eq!(text_count(&output, "Quit Last Light?"), 0);
    for action in [RETRY, OPEN_FOLDER, COPY_PATH] {
        assert_eq!(text_count(&output, action), 1, "{action}");
    }
    // The camp status line under the modal keeps the failure, with its own
    // "Dismiss"; the modal's is the one on top.
    assert_eq!(text_count(&output, DISMISS), 2);
    assert_eq!(modal_primaries(&h), [RETRY]);
    // The save folder is always shown, so it can be copied by hand.
    let folder = f.paths.directory().display().to_string();
    assert_eq!(text_count(&output, &folder), 1, "{folder}");

    // "Open folder" goes through the runner's opener, never the widget.
    // The blocked profile "directory" is a file here, so the runner shows
    // its nearest existing ancestor.
    h.click_topmost(f, View::Full, OPEN_FOLDER);
    assert_eq!(opener.opened(), vec![directory.clone()]);
    assert!(save_modal(f).is_some(), "the modal stays open");
    // "Copy path" puts the folder on the clipboard and says so.
    h.click_topmost(f, View::Full, COPY_PATH);
    assert_eq!(f.status.storage_note, "Path copied.");
    assert!(save_modal(f).is_some());

    // Retry saves again with the same intent; while the cause stands, the
    // modal comes back.
    h.click_topmost(f, View::Full, RETRY);
    assert_eq!(
        save_modal(f).map(|(intent, _)| intent),
        Some(SaveIntent::Manual)
    );
    assert!(
        f.status.storage_note.is_empty(),
        "a reopened modal is fresh"
    );
    // With the cause gone, Retry saves and the modal closes.
    std::fs::remove_file(&blocker).unwrap();
    f.paths = open;
    h.click_topmost(f, View::Full, RETRY);
    assert_eq!(f.status.modal, None, "{}", f.status.message);
    assert!(!f.status.message_error);
    assert!(Game::load(&f.paths.campaign()).is_ok());
    assert!(f.status.toasts.newest(8).any(|toast| toast.text == "Saved"));

    // A failed autosave opens the modal too; its Retry is the player's own
    // save (Autosave becomes Manual), so its result is reported like one.
    f.status.toasts = crate::ui::toast::Toasts::default();
    save_failed(f, SaveIntent::Autosave, "disk full");
    assert_eq!(
        save_modal(f),
        Some((SaveIntent::Autosave, "disk full".into()))
    );
    h.click_topmost(f, View::Full, RETRY);
    assert_eq!(f.status.modal, None, "{}", f.status.message);
    assert!(f.status.toasts.newest(8).any(|toast| toast.text == "Saved"));

    // A failed autosave does not take the modal of a failed save the player
    // asked for: that modal's Retry keeps its intent.
    save_failed(f, SaveIntent::Menu, "disk full");
    save_failed(f, SaveIntent::Autosave, "disk full");
    assert_eq!(
        save_modal(f).map(|(intent, _)| intent),
        Some(SaveIntent::Menu)
    );
    // Dismiss closes it; the same autosave error does not reopen it on every
    // retry, a new error does, and so does a save the player asks for.
    h.click_topmost(f, View::Full, DISMISS);
    assert_eq!(f.status.modal, None);
    save_failed(f, SaveIntent::Autosave, "disk full");
    assert_eq!(f.status.modal, None);
    assert!(f.status.message_error && f.status.message.starts_with("Save failed:"));
    save_failed(f, SaveIntent::Autosave, "Access is denied. (os error 5)");
    assert!(save_modal(f).is_some());
    // Escape dismisses as "Dismiss" does.
    h.key(f, View::Full, Key::Escape);
    assert_eq!(f.status.modal, None);
    save_failed(f, SaveIntent::Manual, "Access is denied. (os error 5)");
    assert!(save_modal(f).is_some());
    // A save a confirmed new journey replaced is not a storage failure.
    f.status.modal = None;
    save_failed(f, SaveIntent::Manual, crate::runner::SAVE_SUPERSEDED);
    assert_eq!(f.status.modal, None);
}

#[test]
fn save_failure_modal_pauses_a_live_battle() {
    let mut fixture = IsolatedFrontend::new();
    fixture.deploy();
    let f = &mut fixture.frontend;
    assert!(f.game.attempt().is_some_and(|a| !a.paused));
    save_failed(f, SaveIntent::Autosave, "disk full");
    assert!(save_modal(f).is_some());
    // The pause is a command, applied by the runner's next pass.
    f.flush();
    assert!(f.game.attempt().is_some_and(|a| a.paused) || f.game.global_paused());
}

#[test]
fn unreadable_settings_modal_names_copy() {
    let fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let directory = fixture.directory.clone();
    std::fs::create_dir_all(&directory).unwrap();
    let original = b"{ this is not json";
    std::fs::write(directory.join("settings.json"), original).unwrap();
    let mut loaded = frontend_loaded_from(&directory);
    let f = &mut loaded;
    let copy = directory.join(crate::profile::UNREADABLE_SETTINGS);
    assert_eq!(std::fs::read(&copy).unwrap(), original);
    let warning = "Settings could not be read; defaults are in use. \
                   The original is kept as settings.unreadable.json.";
    assert_eq!(
        f.status.modal,
        Some(AppModal::Storage(StorageError::Settings {
            warning: warning.into()
        }))
    );
    assert!(f.screen == Screen::Menu);
    let output = shown(&h, f);
    assert_eq!(text_count(&output, SETTINGS_FAILED_TITLE), 1);
    assert_eq!(text_count(&output, warning), 1);
    for action in [OPEN_FOLDER, USE_DEFAULTS] {
        assert_eq!(text_count(&output, action), 1, "{action}");
    }
    for action in [RETRY, RETRY_LOAD, DISMISS] {
        assert_eq!(text_count(&output, action), 0, "{action}");
    }
    assert_eq!(modal_primaries(&h), [USE_DEFAULTS]);
    // "Open folder" shows the profile directory, where the copy is.
    let opener = RecordingOpener::default();
    f.runtime.set_folder_opener(opener.clone());
    h.click_topmost(f, View::Full, OPEN_FOLDER);
    assert_eq!(opener.opened(), vec![directory.clone()]);
    assert!(f.status.modal.is_some());
    // "Use defaults" closes it; the defaults are already in use.
    h.click_topmost(f, View::Full, USE_DEFAULTS);
    assert_eq!(f.status.modal, None);
    assert_eq!(f.settings.master, Settings::default().master);
    assert_eq!(std::fs::read(&copy).unwrap(), original);
    finish(loaded, &directory);
}

#[test]
fn unreadable_save_modal_retries_the_load_until_the_journey_changes() {
    let fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let directory = fixture.directory.clone();
    std::fs::create_dir_all(&directory).unwrap();
    let campaign = directory.join("campaign.json");
    std::fs::write(&campaign, b"not a save").unwrap();
    let mut loaded = frontend_loaded_from(&directory);
    let f = &mut loaded;
    let Some(AppModal::Storage(StorageError::Load {
        error,
        settings: None,
    })) = f.status.modal.clone()
    else {
        panic!(
            "an unreadable save opens the storage modal: {:?}",
            f.status.modal
        )
    };
    assert!(!error.is_empty());
    let output = shown(&h, f);
    assert_eq!(text_count(&output, LOAD_FAILED_TITLE), 1);
    for action in [RETRY_LOAD, OPEN_FOLDER, COPY_PATH, CONTINUE_FRESH] {
        assert_eq!(text_count(&output, action), 1, "{action}");
    }
    assert_eq!(modal_primaries(&h), [RETRY_LOAD]);
    // Still unreadable: the modal stays, with the new error.
    h.click_topmost(f, View::Full, RETRY_LOAD);
    assert!(matches!(
        f.status.modal,
        Some(AppModal::Storage(StorageError::Load { .. }))
    ));
    assert_eq!(f.game.campaign().coins, Game::default().campaign().coins);
    // The player repairs the file (here: a readable save takes its place).
    let mut repaired = Game::default();
    repaired
        .campaign_mut()
        .choose_survivor(SurvivorId::June)
        .unwrap();
    repaired.campaign_mut().coins = 77;
    std::fs::remove_file(&campaign).unwrap();
    repaired.save(&campaign).unwrap();
    h.click_topmost(f, View::Full, RETRY_LOAD);
    assert_eq!(f.status.modal, None, "{}", f.status.message);
    assert_eq!(f.game.campaign().coins, 77);
    assert!(f.status.profile_exists);
    assert_eq!(f.campaign.preview_survivor, SurvivorId::June);
    assert!(!f.status.message_error);
    finish(loaded, &directory);

    // Once the fresh journey is played, the save is not read again: "Retry
    // load" is disabled and "Continue with a fresh journey" is the way on.
    std::fs::write(&campaign, b"not a save").unwrap();
    let mut loaded = frontend_loaded_from(&directory);
    let f = &mut loaded;
    assert!(f.runtime.reload_allowed());
    f.status.modal = None;
    f.ctx()
        .dispatch(Command::ChooseSurvivor(SurvivorId::Rook), "");
    f.flush_for_test();
    assert!(!f.runtime.reload_allowed());
    let played = serde_json::to_value(&f.game).unwrap();
    f.status.modal = Some(AppModal::Storage(StorageError::Load {
        error: "unreadable".into(),
        settings: Some("Settings fell back.".into()),
    }));
    h.click_topmost(f, View::Full, RETRY_LOAD);
    assert!(matches!(
        f.status.modal,
        Some(AppModal::Storage(StorageError::Load { .. }))
    ));
    assert_eq!(serde_json::to_value(&f.game).unwrap(), played);
    // Closing the load failure shows the settings warning of the same
    // startup, which shared the one slot.
    h.click_topmost(f, View::Full, CONTINUE_FRESH);
    assert_eq!(
        f.status.modal,
        Some(AppModal::Storage(StorageError::Settings {
            warning: "Settings fell back.".into()
        }))
    );
    finish(loaded, &directory);
}
