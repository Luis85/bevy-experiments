//! Expedition notices (design-ux-flow §4 "Expedition notices", UX-17, Ruling
//! D10): every settled expedition the player has not acknowledged becomes
//! exactly one toast and a Home "Recent reports" line. This module only
//! reads `Game::unacknowledged_outcomes` and queues
//! `Command::AcknowledgeOutcomes` in the frame that toasts them; core owns
//! which outcomes are pending.
use super::{
    Screen, UiCtx,
    toast::{STACK_VISIBLE, TOAST_LONG_SECS, ToastKind},
};
use horde_core::{
    command::Command,
    sim::{ExpeditionSummary, Game, Outcome},
    world::world,
};

/// One outcome's line: "<Recruit> liberated <Country> · N new fronts" for a
/// liberation, otherwise what the recruit did there and the coins it earned.
/// It never mentions loot or auto-sell.
pub(super) fn outcome_line(recruit: &str, country: &str, summary: &ExpeditionSummary) -> String {
    if summary.liberated {
        return match summary.fronts.len() {
            0 => format!("{recruit} liberated {country}"),
            1 => format!("{recruit} liberated {country} · 1 new front"),
            n => format!("{recruit} liberated {country} · {n} new fronts"),
        };
    }
    let did = match summary.outcome {
        Some(Outcome::Victory) => "held",
        Some(Outcome::Defeat) => "was driven back from",
        Some(Outcome::Withdrawal) => "retreated from",
        None => "returned from",
    };
    match summary.coins {
        0 => format!("{recruit} {did} {country}"),
        coins => format!("{recruit} {did} {country} · +{coins} coins"),
    }
}

/// The one summary toast for outcomes pruning folded into a count.
pub(super) fn folded_line(count: u32) -> String {
    match count {
        1 => "1 more expedition outcome · Operations".into(),
        n => format!("{n} more expedition outcomes · Operations"),
    }
}

/// A won expedition is a Success toast; every other outcome is Info.
fn kind(summary: &ExpeditionSummary) -> ToastKind {
    if summary.liberated || summary.outcome == Some(Outcome::Victory) {
        ToastKind::Success
    } else {
        ToastKind::Info
    }
}

/// How many toasts the current screen shows at once, or `None` where notices
/// wait: a live unpaused defense, and the screens outside the journey (main
/// menu, survivor selection, settings).
fn visible_slots(screen: Screen, game: &Game) -> Option<usize> {
    match screen {
        Screen::Camp | Screen::Results | Screen::History => Some(STACK_VISIBLE),
        Screen::Battle if game.attempt().is_none_or(|a| a.paused) => Some(1),
        _ => None,
    }
}

/// Toasts the pending outcomes that fit the visible toast slots, oldest
/// first, and queues their acknowledgement in the same frame. Outcomes that
/// do not fit stay unacknowledged in core and arrive as the slots free, so
/// none expires unseen behind another toast. Runs before the toast clock
/// advances: a notice is painted in the frame that read it.
pub(super) fn deliver(f: &mut UiCtx) {
    // An acknowledgement the runner has not applied yet: core still lists
    // what this module already toasted. The runner drains its whole queue in
    // one pass, so an empty queue means it was applied.
    if f.status.outcome_ack_pending && f.runtime.pending_commands() > 0 {
        return;
    }
    f.status.outcome_ack_pending = false;
    let Some(visible) = visible_slots(f.screen, f.game) else {
        return;
    };
    let outcomes = f.game.unacknowledged_outcomes();
    if outcomes.notices.is_empty() && outcomes.folded == 0 {
        return;
    }
    let toasts = &f.status.toasts;
    let mut free = visible.saturating_sub(toasts.len());
    let mut batch: Vec<(u64, String, ToastKind)> = Vec::new();
    for notice in &outcomes.notices {
        if free == 0 {
            break;
        }
        let line = outcome_line(&notice.recruit_name, &notice.country_name, &notice.summary);
        // The same text would refresh a shown toast instead of adding one:
        // this outcome waits for that toast to go.
        if toasts.contains(&line) || batch.iter().any(|(_, queued, _)| *queued == line) {
            break;
        }
        batch.push((notice.battle, line, kind(&notice.summary)));
        free -= 1;
    }
    let folded = (free > 0 && outcomes.folded > 0 && batch.len() == outcomes.notices.len())
        .then(|| folded_line(outcomes.folded))
        .filter(|line| !toasts.contains(line));
    if batch.is_empty() && folded.is_none() {
        return;
    }
    let command = Command::AcknowledgeOutcomes {
        battles: batch.iter().map(|(battle, _, _)| *battle).collect(),
        folded: if folded.is_some() { outcomes.folded } else { 0 },
    };
    // Refused while a durable transition runs: nothing is toasted, and the
    // outcomes are read again on a later frame.
    if !f.runtime.enqueue(command, "") {
        return;
    }
    f.status.outcome_ack_pending = true;
    for (_, line, kind) in batch {
        f.status.toasts.push_for(line, kind, TOAST_LONG_SECS);
    }
    if let Some(line) = folded {
        f.status
            .toasts
            .push_for(line, ToastKind::Info, TOAST_LONG_SECS);
    }
}

/// The newest `count` settled expeditions' outcome lines, newest first, for
/// Home "Recent reports". Expeditions settled before save version 7 carry no
/// summary and have no line.
pub(super) fn recent_lines(game: &Game, count: usize) -> Vec<String> {
    game.battles()
        .iter()
        .rev()
        .filter(|battle| battle.settled)
        .filter_map(|battle| {
            let summary = battle.summary.as_ref()?;
            let country = world()
                .country(&battle.country_id)
                .map_or(battle.country_id.as_str(), |c| c.name.as_str());
            Some(outcome_line(
                &battle.game().campaign().npc.name,
                country,
                summary,
            ))
        })
        .take(count)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(outcome: Outcome) -> ExpeditionSummary {
        ExpeditionSummary {
            outcome: Some(outcome),
            ..ExpeditionSummary::default()
        }
    }

    #[test]
    fn outcome_lines_name_the_recruit_the_country_and_the_result() {
        let mut liberated = summary(Outcome::Victory);
        liberated.liberated = true;
        liberated.coins = 90;
        liberated.loot_count = 4;
        assert_eq!(
            outcome_line("Ada Voss", "Belgium", &liberated),
            "Ada Voss liberated Belgium"
        );
        liberated.fronts = vec!["NLD".into()];
        assert_eq!(
            outcome_line("Ada Voss", "Belgium", &liberated),
            "Ada Voss liberated Belgium · 1 new front"
        );
        liberated.fronts.push("LUX".into());
        assert_eq!(
            outcome_line("Ada Voss", "Belgium", &liberated),
            "Ada Voss liberated Belgium · 2 new fronts"
        );
        assert_eq!(kind(&liberated), ToastKind::Success);

        let mut held = summary(Outcome::Victory);
        held.coins = 35;
        held.loot_count = 2;
        assert_eq!(
            outcome_line("Ada Voss", "Poland", &held),
            "Ada Voss held Poland · +35 coins"
        );
        assert_eq!(kind(&held), ToastKind::Success);

        let mut lost = summary(Outcome::Defeat);
        lost.coins = 12;
        assert_eq!(
            outcome_line("Ada Voss", "Poland", &lost),
            "Ada Voss was driven back from Poland · +12 coins"
        );
        assert_eq!(kind(&lost), ToastKind::Info);
        assert_eq!(
            outcome_line("Ada Voss", "Poland", &summary(Outcome::Withdrawal)),
            "Ada Voss retreated from Poland"
        );
        // Loot and auto-sell are never part of a notice.
        for s in [&liberated, &held, &lost] {
            let line = outcome_line("Ada Voss", "Poland", s).to_lowercase();
            assert!(!line.contains("sold") && !line.contains("loot"), "{line}");
        }
    }

    #[test]
    fn the_folded_line_counts_in_singular_and_plural() {
        assert_eq!(folded_line(1), "1 more expedition outcome · Operations");
        assert_eq!(folded_line(7), "7 more expedition outcomes · Operations");
    }
}
