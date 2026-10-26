//! The inspector: one unit's sim-side state, read through a [`SimQuery`] (ADR-0008). The
//! client never reads sim components.
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use fl_protocol::SimId;
use fl_view::{SimQuery, SimReply};

use crate::link::{PendingReplies, RunnerLink, send_query};

/// The unit being inspected, the query in flight and the last reply.
#[derive(Resource, Default)]
pub struct InspectorState {
    pub sim_id: Option<SimId>,
    /// The id of the query whose reply is awaited.
    pub pending: Option<u64>,
    /// The last reply: the entity as JSON, `null` when it doesn't exist, or `{"error": …}`.
    pub last: Option<serde_json::Value>,
}

/// Sends `SimQuery::Entity(sim_id)` and makes `sim_id` the inspected unit; returns the query id.
///
/// # Errors
///
/// If no match is running.
pub fn inspect(world: &mut World, sim_id: SimId) -> Result<u64, String> {
    if !world.contains_resource::<RunnerLink>() {
        return Err("no match is running".to_owned());
    }
    world.init_resource::<PendingReplies>();
    world.init_resource::<InspectorState>();
    Ok(
        world.resource_scope(|world, mut replies: Mut<PendingReplies>| {
            world.resource_scope(|world, mut state: Mut<InspectorState>| {
                start_query(
                    world.resource::<RunnerLink>(),
                    &mut replies,
                    &mut state,
                    sim_id,
                )
            })
        }),
    )
}

fn start_query(
    link: &RunnerLink,
    replies: &mut PendingReplies,
    state: &mut InspectorState,
    sim_id: SimId,
) -> u64 {
    let id = send_query(link, replies, SimQuery::Entity(sim_id));
    state.sim_id = Some(sim_id);
    state.pending = Some(id);
    state.last = None;
    id
}

/// The JSON the inspector shows for `reply`.
fn reply_json(reply: SimReply) -> serde_json::Value {
    match reply {
        SimReply::Entity(Some(value)) => value,
        SimReply::Entity(None) => serde_json::Value::Null,
        SimReply::Error(message) => serde_json::json!({ "error": message }),
        other => serde_json::json!({ "error": format!("unexpected reply: {other:?}") }),
    }
}

/// Moves the awaited reply, once it has arrived, into [`InspectorState::last`].
pub(crate) fn receive_inspector_reply(
    mut state: ResMut<InspectorState>,
    mut replies: ResMut<PendingReplies>,
) {
    let Some(id) = state.pending else {
        return;
    };
    if let Some(reply) = replies.take(id) {
        state.pending = None;
        state.last = Some(reply_json(reply));
    }
}

/// Draws the inspector: a unit id field, an Inspect button and the last reply.
pub(crate) fn inspector_window(
    mut contexts: EguiContexts,
    mut state: ResMut<InspectorState>,
    mut replies: ResMut<PendingReplies>,
    link: Option<Res<RunnerLink>>,
    mut draft: Local<u32>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    let mut requested = None;
    egui::Window::new("Inspector")
        .default_open(false)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("sim id");
                ui.add(egui::DragValue::new(&mut *draft));
                let button = ui.add_enabled(link.is_some(), egui::Button::new("Inspect"));
                if button.clicked() {
                    requested = Some(SimId(*draft));
                }
            });
            if let Some(sim_id) = state.sim_id {
                ui.label(format!("unit {sim_id}"));
            }
            if state.pending.is_some() {
                ui.label("waiting for the reply…");
            }
            match &state.last {
                None => {}
                Some(serde_json::Value::Null) => {
                    ui.label("no such unit");
                }
                Some(value) => {
                    let text = serde_json::to_string_pretty(value).unwrap_or_default();
                    egui::ScrollArea::vertical()
                        .max_height(320.0)
                        .show(ui, |ui| ui.monospace(text));
                }
            }
        });
    if let (Some(sim_id), Some(link)) = (requested, link) {
        start_query(&link, &mut replies, &mut state, sim_id);
    }
    Ok(())
}
