//! "Save behaviour to file" and "Reload behaviour from file" (the resident
//! card's Why page). The view asks; the host does the file work and calls
//! back here: the document is checked by the simulation's own rule and then
//! travels whole in one recorded `ReplaceBehavior` command, so a replay needs
//! no file. Nothing here touches simulation state.
use bevy::prelude::*;
use island_content::village::resident::{BehaviorSpec, replace};
use npc_framework::prelude::{SimCommand, submit};

/// What the player asked for; the host takes it and does the file work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BehaviourFile {
    Save,
    Reload,
}

/// The view's request to the host (`None`: nothing asked).
#[derive(Resource, Default)]
pub struct BehaviourFileRequest(pub Option<BehaviourFile>);

/// Take the player's request, once.
pub fn take_behaviour_request(world: &mut World) -> Option<BehaviourFile> {
    world
        .get_resource_mut::<BehaviourFileRequest>()
        .and_then(|mut request| request.0.take())
}

/// Ask the host for a save or a reload.
pub(crate) fn request(world: &mut World, what: BehaviourFile) {
    world.get_resource_or_init::<BehaviourFileRequest>().0 = Some(what);
}

/// The running behaviour document as the file holds it (pretty JSON).
pub fn behaviour_text(world: &World) -> Option<String> {
    replace::export_behavior(world).and_then(|spec| serde_json::to_string_pretty(&spec).ok())
}

/// What the confirmation says once the command is sent.
pub const CHANGED: &str = "Behaviour changed for the whole island.";

/// What reloading says when the file holds the behaviour already in force.
pub const UNCHANGED: &str = "Nothing changed: the file holds the behaviour the island follows now.";

/// Read a behaviour document, check it with the simulation's rule and send
/// it as one recorded command. `Err` is one plain sentence and nothing is
/// sent: a file that is not a behaviour document, or the simulation's own
/// refusal ("Behaviour not changed: ...").
pub fn reload_from_text(world: &mut World, text: &str) -> Result<&'static str, String> {
    let spec: BehaviorSpec =
        serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|error| {
            format!(
                "Behaviour not changed: the file is not a behaviour document.
Details: {error}"
            )
        })?;
    // The same document as the island's own: nothing is sent.
    let same = replace::export_behavior(world)
        .is_some_and(|now| serde_json::to_value(&now).ok() == serde_json::to_value(&spec).ok());
    if same {
        return Ok(UNCHANGED);
    }
    replace::check_behavior(world, &spec)?;
    submit(
        world,
        None,
        SimCommand::content(&replace::ReplaceBehavior { behavior: spec }),
    )
    .map_err(|error| format!("Behaviour not changed: {error}"))?;
    Ok(CHANGED)
}

#[cfg(test)]
mod tests {
    use super::*;
    use npc_framework::kernel::commands::CommandQueue;

    fn queued(app: &App) -> usize {
        app.world()
            .get_resource::<CommandQueue>()
            .map_or(0, |queue| queue.pending.len())
    }

    /// Raise the first positive integer of the document by one.
    fn change_one_number(value: &mut serde_json::Value) -> bool {
        match value {
            serde_json::Value::Number(number) => match number.as_u64() {
                Some(n) if n > 0 => {
                    *value = serde_json::Value::from(n + 1);
                    true
                }
                _ => false,
            },
            serde_json::Value::Array(items) => items.iter_mut().any(change_one_number),
            serde_json::Value::Object(map) => map.values_mut().any(change_one_number),
            _ => false,
        }
    }

    #[test]
    fn a_bad_file_says_why_and_a_good_one_sends_one_command() {
        let mut app = island_content::new_sim_app(7, "living_world");
        let error = reload_from_text(app.world_mut(), "not json").unwrap_err();
        assert!(error.starts_with("Behaviour not changed: "), "{error}");
        assert!(
            error.contains(
                ".
Details: "
            ),
            "the error on its own line: {error}"
        );
        assert_eq!(queued(&app), 0, "nothing is sent");
        let text = behaviour_text(app.world()).expect("the island's behaviour");
        assert_eq!(reload_from_text(app.world_mut(), &text), Ok(UNCHANGED));
        assert_eq!(queued(&app), 0, "the same behaviour sends nothing");
        let mut spec: serde_json::Value = serde_json::from_str(&text).unwrap();
        let changed = change_one_number(&mut spec);
        assert!(changed, "the document has a number to change");
        let text = serde_json::to_string(&spec).unwrap();
        match reload_from_text(app.world_mut(), &text) {
            Ok(done) => {
                assert_eq!(done, CHANGED);
                assert_eq!(queued(&app), 1, "exactly one command");
            }
            // A changed number the simulation refuses still sends nothing.
            Err(refused) => {
                assert!(refused.starts_with("Behaviour not changed"), "{refused}");
                assert_eq!(queued(&app), 0);
            }
        }
        let mut request = BehaviourFileRequest::default();
        assert!(request.0.take().is_none());
        app.insert_resource(BehaviourFileRequest(Some(BehaviourFile::Reload)));
        assert_eq!(
            take_behaviour_request(app.world_mut()),
            Some(BehaviourFile::Reload)
        );
        assert_eq!(take_behaviour_request(app.world_mut()), None);
    }
}
