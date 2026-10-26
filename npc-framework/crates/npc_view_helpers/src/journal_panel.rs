//! Journal panel (F3 in the village app): the last [`JOURNAL_ROWS`] journal
//! entries, filtered to the selected unit (else all), newest at the bottom.
//! Content effects are described by the [`EffectDescriber`] resource when
//! the content installs one, else shown as their JSON.

use std::fmt::Write as _;

use bevy::prelude::*;
use npc_kernel::JsonObject;
use npc_kernel::journal::{Journal, JournalEntry, JournalFilter, JournalKind, query};

use crate::select::Selected;
use crate::{DebugPanels, ViewSystems};

/// Content hook: a one-line description of an `Effect` journal entry's
/// effect (`None`: fall back to its JSON).
#[derive(Resource, Clone, Copy)]
pub struct EffectDescriber(pub fn(&JsonObject) -> Option<String>);

/// Rows the panel shows.
pub const JOURNAL_ROWS: usize = 30;
/// Characters per row (longer rows are cut).
const ROW_CHARS: usize = 76;

#[derive(Component)]
struct JournalText;

/// What the text was built from; rebuilt only when it changes.
#[derive(Default, PartialEq)]
struct Shown {
    visible: bool,
    unit: Option<npc_kernel::SimId>,
    enabled: bool,
    last: Option<(u64, u32)>,
    len: usize,
}

pub(crate) fn plugin(app: &mut App) {
    app.add_systems(Startup, spawn_panel)
        .add_systems(Update, update_panel.in_set(ViewSystems::Present));
}

fn spawn_panel(mut commands: Commands) {
    commands.spawn((
        Name::new("journal panel"),
        JournalText,
        Text::new(""),
        TextFont {
            font_size: FontSize::Px(12.0),
            ..default()
        },
        TextColor(Color::srgb(0.92, 0.95, 0.9)),
        BackgroundColor(Color::srgba(0.05, 0.06, 0.08, 0.78)),
        Node {
            position_type: PositionType::Absolute,
            top: px(10),
            right: px(10),
            max_width: px(640),
            padding: UiRect::all(px(8)),
            ..default()
        },
        Visibility::Hidden,
    ));
}

fn on_off(b: bool, yes: &str, no: &str) -> String {
    if b { yes.to_owned() } else { no.to_owned() }
}

fn effect(e: &JsonObject, describer: Option<&EffectDescriber>) -> String {
    describer
        .and_then(|d| (d.0)(e))
        .unwrap_or_else(|| serde_json::to_string(e).unwrap_or_default())
}

/// One-line, human-readable form of a journal kind.
pub fn describe(kind: &JournalKind) -> String {
    describe_with(kind, None)
}

/// [`describe`] with the content's effect describer.
pub fn describe_with(kind: &JournalKind, describer: Option<&EffectDescriber>) -> String {
    match kind {
        JournalKind::ThinkStart { reason } => format!("think ({reason:?})"),
        JournalKind::LeafStart {
            action,
            status,
            path,
            ..
        } => {
            format!("start {action} {status:?}  [{path}]")
        }
        JournalKind::LeafStop { action, reason, .. } => format!("stop  {action} ({reason:?})"),
        JournalKind::Abort { guard, aborted } => {
            format!("abort by {guard}: {}", aborted.join(" > "))
        }
        JournalKind::CondEval { cond, result, .. } => format!("cond  {cond} = {result}"),
        JournalKind::RootDone { status } => format!("root  {status:?}"),
        JournalKind::NeedLevel { need, from, to } => format!("{need:?} {from:?} -> {to:?}"),
        JournalKind::Reserve { target, ok } => {
            format!("reserve #{} {}", target.0, on_off(*ok, "ok", "REFUSED"))
        }
        JournalKind::Release { target, reason } => format!("release #{} ({reason:?})", target.0),
        JournalKind::Effect { effect: e, applied } => {
            format!(
                "effect {}{}",
                effect(e, describer),
                on_off(*applied, "", " (dropped)")
            )
        }
        JournalKind::Command { cmd } => format!(
            "COMMAND {}",
            serde_json::to_string(cmd).unwrap_or_else(|_| format!("{cmd:?}"))
        ),
        JournalKind::PhaseChange { night } => on_off(*night, "night falls", "day breaks"),
        JournalKind::Spawn { kind } => format!("spawn {kind}"),
        JournalKind::Despawn => "despawn".to_owned(),
        JournalKind::PathFound { len, nodes } => format!("path {len} waypoints ({nodes} nodes)"),
        JournalKind::PathFailed { reason } => format!("PATH FAILED ({reason:?})"),
        JournalKind::Roll {
            skill,
            target,
            roll,
            margin,
            success,
            crit,
            purpose,
        } => format!(
            "roll {skill} {roll} vs {target} ({}{margin:+}{}) [{purpose}]",
            on_off(*success, "ok ", "FAIL "),
            match crit {
                npc_kernel::journal::Critical::None => "",
                npc_kernel::journal::Critical::Success => ", CRIT",
                npc_kernel::journal::Critical::Failure => ", CRIT FAIL",
            }
        ),
        JournalKind::Perceived { target, via } => format!("perceived #{} ({via})", target.0),
        JournalKind::Lost { target, forgotten } => format!(
            "lost #{}{}",
            target.0,
            on_off(*forgotten, " (forgotten)", "")
        ),
        JournalKind::ControllerAttached {
            controller,
            mode,
            on_idle,
        } => format!("AGENT {controller} attached ({mode}, idle: {on_idle})"),
        JournalKind::ControllerDetached { controller } => format!("AGENT {controller} detached"),
        JournalKind::ExternalAction {
            handle,
            verb,
            status,
            ..
        } => format!("agent action #{handle} {verb} {status}"),
        JournalKind::ActionRejected {
            controller,
            verb,
            reason,
        } => format!("agent {controller}: {verb} REJECTED ({reason})"),
        JournalKind::Interacted {
            verb,
            target,
            outcome,
            reason,
            ..
        } => match reason {
            Some(r) => format!("{verb} #{} {outcome} ({r})", target.0),
            None => format!("{verb} #{} {outcome}", target.0),
        },
        JournalKind::ItemMoved {
            item,
            kind,
            from,
            to,
            slot,
            ..
        } => {
            let place = |p: &Option<npc_kernel::SimId>| {
                p.map_or("ground".to_owned(), |h| format!("#{}", h.0))
            };
            match slot {
                Some(s) => format!("item {kind} #{} -> {} {s}", item.0, place(to)),
                None => format!("item {kind} #{} {} -> {}", item.0, place(from), place(to)),
            }
        }
        JournalKind::DoorChanged { door, open, locked } => format!(
            "door #{} {}{}",
            door.0,
            on_off(*open, "open", "closed"),
            on_off(*locked, ", locked", "")
        ),
        JournalKind::Mounted {
            vehicle,
            seat,
            role,
        } => format!("mounted #{} seat {seat} ({role})", vehicle.0),
        JournalKind::Dismounted { vehicle, seat, at } => format!(
            "dismounted #{} seat {seat} at ({}, {})",
            vehicle.0, at.x, at.z
        ),
        JournalKind::VehicleStalled { reason } => format!("vehicle STALLED ({reason})"),
        JournalKind::XpGained { source, amount, xp } => {
            format!("+{amount} xp ({source}), total {xp}")
        }
        JournalKind::LevelUp { level, points } => format!("LEVEL {level} (+{points} points)"),
        JournalKind::SkillImproved {
            skill,
            points,
            level,
            via,
        } => format!("{skill} +{points} pt ({via}) -> {level}"),
        JournalKind::CraftStarted {
            recipe,
            workstation,
            ends,
        } => match workstation {
            Some(w) => format!("craft {recipe} at #{} until {ends}", w.0),
            None => format!("craft {recipe} until {ends}"),
        },
        JournalKind::CraftDone {
            recipe,
            quality,
            margin,
            outputs,
            ..
        } => format!(
            "craft {recipe} done: q{quality} (by {margin}), {} item(s)",
            outputs.len()
        ),
        JournalKind::CraftFailed {
            recipe,
            outcome,
            reason,
            ..
        } => format!("craft {recipe} {outcome} ({reason})"),
        JournalKind::OrderIssued {
            order,
            queued,
            player,
        } => format!(
            "order {order} from player {player}{}",
            on_off(*queued, " (queued)", "")
        ),
        JournalKind::OrderStarted { order } => format!("order {order} started"),
        JournalKind::OrderDone { order } => format!("order {order} done"),
        JournalKind::OrderFailed { order, reason, .. } => {
            format!("order {order} FAILED ({reason})")
        }
    }
}

/// `  tick.seq #unit  description`, cut to the panel width.
pub fn format_entry(entry: &JournalEntry) -> String {
    format_entry_with(entry, None)
}

/// [`format_entry`] with the content's effect describer.
pub fn format_entry_with(entry: &JournalEntry, describer: Option<&EffectDescriber>) -> String {
    let unit = entry
        .unit
        .map_or_else(|| "  -".to_owned(), |u| format!("#{:<2}", u.0));
    let line = format!(
        "{:>6}.{:<2} {unit} {}",
        entry.tick,
        entry.seq,
        describe_with(&entry.kind, describer)
    );
    if line.chars().count() > ROW_CHARS {
        let mut cut: String = line.chars().take(ROW_CHARS - 2).collect();
        cut.push_str("..");
        cut
    } else {
        line
    }
}

fn update_panel(
    panels: Res<DebugPanels>,
    selected: Res<Selected>,
    journal: Option<Res<Journal>>,
    describer: Option<Res<EffectDescriber>>,
    mut shown: Local<Shown>,
    mut texts: Query<(&mut Text, &mut Visibility), With<JournalText>>,
) {
    let now = Shown {
        visible: panels.journal,
        unit: selected.0,
        enabled: journal.as_ref().is_some_and(|j| j.enabled),
        last: journal
            .as_ref()
            .and_then(|j| j.entries.back())
            .map(|e| (e.tick, e.seq)),
        len: journal.as_ref().map_or(0, |j| j.entries.len()),
    };
    if *shown == now {
        return;
    }
    let want = if now.visible {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    let s = if now.visible {
        build(journal.as_deref(), selected.0, describer.as_deref())
    } else {
        String::new()
    };
    for (mut text, mut vis) in &mut texts {
        if *vis != want {
            *vis = want;
        }
        if now.visible && text.0 != s {
            text.0.clone_from(&s);
        }
    }
    *shown = now;
}

fn build(
    journal: Option<&Journal>,
    unit: Option<npc_kernel::SimId>,
    describer: Option<&EffectDescriber>,
) -> String {
    let mut s = String::new();
    let who = unit.map_or_else(|| "all units".to_owned(), |u| format!("unit #{}", u.0));
    let _ = writeln!(s, "[F3] journal - {who} (click a unit to filter)");
    let Some(journal) = journal else {
        s.push_str("  no journal resource");
        return s;
    };
    if !journal.enabled {
        s.push_str("  journal is off (JournalPlugin turns it on)\n");
    }
    let filter = JournalFilter {
        unit,
        limit: Some(JOURNAL_ROWS),
        ..default()
    };
    let rows = query(journal, &filter);
    if rows.is_empty() {
        s.push_str("  (no entries yet)");
    }
    for (i, entry) in rows.iter().enumerate() {
        if i > 0 {
            s.push('\n');
        }
        s.push_str(&format_entry_with(entry, describer));
    }
    s
}

#[cfg(test)]
mod tests {
    use npc_kernel::SimId;
    use npc_kernel::commands::SimCommand;
    use npc_kernel::journal::{Journal, JournalKind};

    use super::*;

    #[test]
    fn rows_filter_to_unit_and_cap() {
        let mut journal = Journal::default();
        journal.enabled = true;
        for t in 0..40 {
            journal.push(
                t,
                Some(SimId(5)),
                JournalKind::Spawn {
                    kind: "villager".into(),
                },
            );
            journal.push(t, None, JournalKind::PhaseChange { night: t % 2 == 0 });
        }
        journal.push(
            41,
            Some(SimId(5)),
            JournalKind::Command {
                cmd: SimCommand::Despawn { unit: 5 },
            },
        );
        let all = build(Some(&journal), None, None);
        assert_eq!(all.lines().count(), 1 + JOURNAL_ROWS, "{all}");
        let mine = build(Some(&journal), Some(SimId(5)), None);
        assert!(mine.lines().skip(1).all(|l| l.contains("#5")), "{mine}");
        assert!(
            mine.ends_with(r#"COMMAND {"type":"Despawn","unit":5}"#),
            "{mine}"
        );
        assert!(build(None, None, None).contains("no journal"));
    }

    #[test]
    fn long_rows_are_cut() {
        let entry = JournalEntry {
            tick: 7,
            seq: 1,
            unit: Some(SimId(2)),
            kind: JournalKind::Spawn {
                kind: "x".repeat(200),
            },
        };
        let line = format_entry(&entry);
        assert_eq!(line.chars().count(), ROW_CHARS);
        assert!(line.starts_with("     7.1  #2 "), "{line}");
    }
}
