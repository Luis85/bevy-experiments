//! Read-only public actor feedback shared by observer projections and native views.
//! Never reads notebook stores or arbitrary action arguments/rejection messages.
use super::{Villager, speech};
use bevy_ecs::prelude::*;
use npc_framework::agent_io::registry::AgentRegistry;
use npc_framework::agent_io::stats::ActionNotices;
use npc_framework::kernel::journal::Journal;
use npc_framework::prelude::*;
use serde::Serialize;
use std::collections::BTreeMap;

pub const THOUGHT_TICKS: u64 = 160;
pub const SPEECH_TICKS: u64 = 360;
const MAX_ACTIONS: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BubbleKind {
    Thought,
    Speech,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PublicBubble {
    pub kind: BubbleKind,
    pub text: String,
    pub tick: u64,
    pub expires_tick: u64,
}

/// A trusted registration key, never a submitted free-form label. Unknown or
/// malformed names receive a closed fallback at the calling site.
pub fn public_verb(world: &World, candidate: &str) -> Option<&'static str> {
    let name = match candidate {
        "cancel" => "cancel",
        "clear" => "clear",
        _ => {
            *world
                .get_resource::<AgentRegistry>()?
                .handlers
                .get_key_value(candidate)?
                .0
        }
    };
    (!name.chars().any(char::is_control)).then_some(name)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum ActionKey {
    Notice(u64),
    Handle(u32),
    Command(u64, u32),
}
type Thoughts = BTreeMap<SimId, Vec<(ActionKey, u64, String)>>;

fn thought(thoughts: &mut Thoughts, actor: SimId, key: ActionKey, tick: u64, label: &str) {
    let actions = thoughts.entry(actor).or_default();
    let label: String = label.chars().take(40).collect();
    if let Some(index) = actions.iter().position(|(seen, _, _)| *seen == key) {
        let (_, previous_tick, _) = actions.remove(index);
        actions.push((key, tick.max(previous_tick), label));
    } else if matches!(key, ActionKey::Handle(_))
        && actions
            .last()
            .is_some_and(|(seen, previous_tick, previous_label)| {
                matches!(seen, ActionKey::Notice(_))
                    && *previous_tick <= tick
                    && previous_label == &label
            })
    {
        // Acceptance followed by the actual start is one adjacent public
        // intention. Retain the handle so another start refreshes its TTL.
        *actions.last_mut().expect("adjacent notice") = (key, tick, label);
    } else {
        actions.push((key, tick, label));
    }
    actions.sort_by_key(|(key, tick, _)| (*tick, *key));
    if actions.len() > MAX_ACTIONS {
        actions.remove(0);
    }
}

/// Compute a complete replacement once between ticks. No cursors, entities or
/// resources are created, so export frequency cannot change simulation results.
/// The returned operator feed is omniscient; agent observations use heard speech
/// and perception through their separate, restricted interface.
pub fn bubbles(world: &World) -> BTreeMap<SimId, Vec<PublicBubble>> {
    let now = world.resource::<SimClock>().tick;
    let epoch = npc_framework::agent_io::api::epoch(world);
    let index = world.resource::<SimIndex>();
    let is_actor = |id| {
        index
            .get(id)
            .is_some_and(|e| world.get::<Villager>(e).is_some())
    };
    let active = |tick: u64, duration| tick <= now && tick.saturating_add(duration) > now;
    let mut thoughts = Thoughts::new();
    let mut spoken = BTreeMap::<SimId, PublicBubble>::new();
    let notices = world.get_resource::<ActionNotices>();
    let live_notices =
        notices.is_some_and(|notices| notices.entries.iter().any(|n| n.epoch == epoch));
    if let Some(notices) = notices {
        for notice in &notices.entries {
            let actor = SimId(notice.unit);
            if notice.epoch == epoch && active(notice.tick, THOUGHT_TICKS) && is_actor(actor) {
                thought(
                    &mut thoughts,
                    actor,
                    ActionKey::Notice(notice.seq),
                    notice.tick,
                    public_verb(world, &notice.verb).unwrap_or("action"),
                );
            }
        }
    }
    if let Some(journal) = world.get_resource::<Journal>() {
        for entry in &journal.entries {
            let Some(actor) = entry.unit.filter(|id| is_actor(*id)) else {
                continue;
            };
            match &entry.kind {
                JournalKind::Effect {
                    effect,
                    applied: true,
                } if active(entry.tick, SPEECH_TICKS)
                    && effect.get("type").and_then(|v| v.as_str()) == Some("VillageSpeech") =>
                {
                    if let Some(text) = effect.get("text").and_then(|v| v.as_str())
                        && speech::validate_text(text).is_ok()
                    {
                        spoken.insert(
                            actor,
                            PublicBubble {
                                kind: BubbleKind::Speech,
                                text: text.into(),
                                tick: entry.tick,
                                expires_tick: entry.tick.saturating_add(SPEECH_TICKS),
                            },
                        );
                    }
                }
                JournalKind::ExternalAction {
                    handle,
                    verb,
                    status,
                    ..
                } if active(entry.tick, THOUGHT_TICKS)
                    && (status == "started"
                        || (status == "queued"
                            && !notices.is_some_and(|notices| {
                                notices.entries.iter().any(|notice| {
                                    notice.epoch == epoch
                                        && notice.unit == actor.0
                                        && notice.verb == *verb
                                        && notice.tick <= entry.tick
                                        && entry.tick.saturating_sub(notice.tick) < THOUGHT_TICKS
                                })
                            }))) =>
                {
                    thought(
                        &mut thoughts,
                        actor,
                        ActionKey::Handle(*handle),
                        entry.tick,
                        public_verb(world, verb).unwrap_or("recorded action"),
                    );
                }
                JournalKind::Command {
                    cmd: SimCommand::Content { kind, payload },
                } if !live_notices
                    && active(entry.tick, THOUGHT_TICKS)
                    && !matches!(
                        kind.as_str(),
                        "AgentAct"
                            | "AttachController"
                            | "DetachController"
                            | "VillagePublishDiary"
                    )
                    && index.get(actor).is_some_and(|e| {
                        world
                            .get::<npc_framework::agent_io::ExternalController>(e)
                            .is_some()
                    }) =>
                {
                    let label = payload
                        .get("verb")
                        .and_then(|v| v.as_str())
                        .and_then(|verb| public_verb(world, verb))
                        .or_else(|| public_verb(world, kind))
                        .unwrap_or("recorded action");
                    thought(
                        &mut thoughts,
                        actor,
                        ActionKey::Command(entry.tick, entry.seq),
                        entry.tick,
                        label,
                    );
                }
                _ => {}
            }
        }
    }
    let mut result = BTreeMap::<SimId, Vec<PublicBubble>>::new();
    for (actor, actions) in thoughts {
        let tick = actions.last().map_or(now, |(_, tick, _)| *tick);
        let text = format!(
            "Action: {}",
            actions
                .iter()
                .map(|(_, _, label)| label.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        );
        result.entry(actor).or_default().push(PublicBubble {
            kind: BubbleKind::Thought,
            text,
            tick,
            expires_tick: tick.saturating_add(THOUGHT_TICKS),
        });
    }
    for (actor, speech) in spoken {
        result.entry(actor).or_default().push(speech);
    }
    result
}
