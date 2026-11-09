//! Recorded, bounded in-world dialogue. The journal is the ephemeral transcript;
//! no renderer, provider conversation or private notebook is read by speech.
use super::{catalog, economy, model::*};
use bevy_ecs::prelude::*;
use npc_framework::interaction::items::entity_of;
use npc_framework::kernel::journal::Journal;
use npc_framework::prelude::*;
use serde::Serialize;
use serde_json::{Value, json};

pub const AUDIBLE_RADIUS_MM: i32 = 6000;
pub const MAX_SPEECH_CHARS: usize = 240;
pub const MAX_SPEECH_BYTES: usize = 1024;
pub const HEARD_WINDOW_TICKS: u64 = 1200;
pub const MAX_HEARD_LINES: usize = 8;

/// Authored public questions. The UI sends the declared text through ordinary
/// `talk`; these are not commands, quest rewards or access to private facts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ConversationTopic {
    pub id: &'static str,
    pub label: &'static str,
    pub text: &'static str,
}
pub const COLONY_TOPICS: &[ConversationTopic] = &[
    ConversationTopic {
        id: "home",
        label: "Building a home",
        text: "How can I build a home?",
    },
    ConversationTopic {
        id: "garden",
        label: "Growing crops",
        text: "How do I start a garden?",
    },
    ConversationTopic {
        id: "water",
        label: "Finding water",
        text: "Where can I get clean water?",
    },
    ConversationTopic {
        id: "trade",
        label: "Trading with neighbors",
        text: "How can I earn coins here?",
    },
];

fn public_topic_reply(world: &World, text: &str) -> Option<String> {
    if !super::colony::enabled(world) {
        return None;
    }
    let topic = COLONY_TOPICS
        .iter()
        .find(|topic| topic.text.eq_ignore_ascii_case(text.trim()))?;
    Some(match topic.id {
        "home" => format!("Look for a vacant plot along the southern house lane. A cottage needs {} planks and {} logs. Saw logs at the carpenter's workshop, then build at the plot door.",super::homestead::PLANK_COST,super::homestead::LOG_COST),
        "garden" => "Plant grain for milling and trade, or hardy berries for food. Bring a seed or berry from your pack and water the bed once. Keep part of each harvest to plant again.".into(),
        "water" => "The village well supplies clean water. Draw it from the source or buy carried water from the well keeper. Keep some in your pack; planted beds need water too.".into(),
        "trade" => "Offer real surplus or trade with a resident who needs it. Prices and demand change. Coins arrive only after an actual sale; keep food, water and seed for yourself.".into(),
        _ => return None,
    })
}

pub fn validate_text(text: &str) -> Result<(), String> {
    if text.trim().is_empty()
        || text.chars().count() > MAX_SPEECH_CHARS
        || text.len() > MAX_SPEECH_BYTES
        || text
            .chars()
            .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}'))
    {
        return Err("speech needs one nonempty line, at most 240 characters / 1024 bytes, without control characters".into());
    }
    Ok(())
}

pub(super) fn check(
    world: &World,
    actor: Entity,
    target: Option<SimId>,
    text: &str,
) -> Result<(), String> {
    validate_text(text)?;
    if world.get::<Villager>(actor).is_none() || world.get::<SimPos>(actor).is_none() {
        return Err("speech needs a village actor with a position".into());
    }
    if let Some(target) = target {
        let listener = entity_of(world, target).ok_or("speech target is missing")?;
        if actor == listener {
            return Err("omit target to speak aloud instead of addressing yourself".into());
        }
        if !living(world, listener)
            || world.get::<Villager>(listener).is_none()
            || !economy::near(world, actor, listener)
        {
            return Err("move within 1.5 m of a living actor to speak to them".into());
        }
    }
    Ok(())
}

fn emit(world: &mut World, actor: SimId, target: Option<SimId>, text: &str) {
    let Some(speaker) = entity_of(world, actor) else {
        return;
    };
    let Some(position) = world.get::<SimPos>(speaker).copied() else {
        return;
    };
    let mut listeners: Vec<SimId> = if let Some(target) = target {
        vec![target]
    } else {
        let radius = i64::from(AUDIBLE_RADIUS_MM);
        world
            .query::<(Entity, &SimId, &SimPos, &Villager)>()
            .iter(world)
            .filter(|(entity, id, pos, _)| {
                **id != actor && living(world, *entity) && position.dist2(**pos) <= radius * radius
            })
            .map(|(_, id, _, _)| *id)
            .collect()
    };
    listeners.sort_unstable();
    record_effect(
        world,
        Some(actor),
        "VillageSpeech",
        &json!({"text":text,"target":target}),
    );
    for listener in listeners {
        record_effect(
            world,
            Some(listener),
            "VillageSpeechHeard",
            &json!({"speaker":actor,"text":text,"target":target}),
        );
    }
}

/// One addressed resident reply; emitted directly so it cannot recurse or
/// trigger autonomous agent decisions. All offers come from the public catalog.
pub(super) fn speak(world: &mut World, actor: SimId, target: Option<SimId>, text: &str) {
    emit(world, actor, target, text);
    let Some(target) = target else { return };
    let Some(listener) = entity_of(world, target) else {
        return;
    };
    let Some(person) = world.get::<Villager>(listener) else {
        return;
    };
    if person.role == Role::Visitor {
        return;
    }
    let goods: Vec<_> = catalog::outputs_for(world, person.role)
        .iter()
        .map(|kind| match *kind {
            catalog::GRAIN => "grain",
            catalog::FLOUR => "flour",
            catalog::WATER => "clean water",
            catalog::BOWL => "wooden bowls",
            other => other,
        })
        .collect();
    let offer = if let Some(reply) = public_topic_reply(world, text) {
        reply
    } else if person.role == Role::Innkeeper {
        format!("A bed costs {} coins.", catalog::BED_PRICE)
    } else if goods.is_empty() {
        "Welcome to the village.".into()
    } else {
        format!(
            "I sell {}. Ask about a trade when you are ready.",
            goods.join(", ")
        )
    };
    let reply = format!(
        "Hello. I am {}, the {}. {offer}",
        person.name,
        person.role.name()
    );
    if validate_text(&reply).is_ok() {
        emit(world, target, Some(actor), &reply);
    }
}

/// Only this actor's recipient-owned records enter its observation. No current
/// speaker location/name, distant journal stream or private books are consulted.
pub(super) fn heard(world: &World, actor: SimId) -> Vec<Value> {
    let Some(journal) = world.get_resource::<Journal>() else {
        return Vec::new();
    };
    let now = world.resource::<SimClock>().tick;
    let mut lines: Vec<_> = journal.entries.iter().rev()
        .filter(|entry| entry.unit == Some(actor) && now.saturating_sub(entry.tick) <= HEARD_WINDOW_TICKS)
        .filter_map(|entry| {
            let JournalKind::Effect { effect, applied: true } = &entry.kind else { return None };
            if effect.get("type")?.as_str()? != "VillageSpeechHeard" { return None; }
            let text = effect.get("text")?.as_str()?;
            validate_text(text).ok()?;
            let speaker = u32::try_from(effect.get("speaker")?.as_u64()?).ok()?;
            Some(json!({"tick":entry.tick,"speaker":speaker,"text":text,"target":effect.get("target")}))
        }).take(MAX_HEARD_LINES).collect();
    lines.reverse();
    lines
}
