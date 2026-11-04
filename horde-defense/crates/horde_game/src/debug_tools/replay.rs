//! Headless replay: rebuilds the recorded `Game` from the header and feeds it
//! the recorded commands and inputs. No Bevy `App` is involved.
use super::recording::{FORMAT, Header, Line, hex};
use horde_core::{
    command::Command,
    sim::{Game, Input, RULESET_VERSION, SAVE_VERSION},
};
use std::{io::BufRead, path::Path};

#[derive(Debug)]
pub struct ReplayReport {
    pub ticks: u64,
    pub commands: usize,
    pub digest: u64,
    /// The digest the recording ended with, when it was closed properly.
    pub recorded_digest: Option<String>,
    pub warnings: Vec<String>,
}
impl ReplayReport {
    /// False only when the recording stored a digest and the replay differs.
    pub fn matches_recording(&self) -> bool {
        self.recorded_digest
            .as_ref()
            .is_none_or(|recorded| *recorded == hex(self.digest))
    }
}

pub fn read_lines(path: &Path) -> Result<Vec<Line>, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut lines = Vec::new();
    for (index, raw) in std::io::BufReader::new(file).lines().enumerate() {
        let raw = raw.map_err(|e| format!("{}: {e}", path.display()))?;
        if raw.trim().is_empty() {
            continue;
        }
        lines.push(
            serde_json::from_str(&raw)
                .map_err(|e| format!("{}:{}: {e}", path.display(), index + 1))?,
        );
    }
    Ok(lines)
}

pub fn replay_file(path: &Path) -> Result<ReplayReport, String> {
    replay(read_lines(path)?)
}

fn compatibility_warnings(header: &Header, game: &Game) -> Vec<String> {
    let mut warnings = Vec::new();
    if header.format != FORMAT {
        warnings.push(format!(
            "recording format {} differs from this build's {FORMAT}",
            header.format
        ));
    }
    if header.version != SAVE_VERSION {
        warnings.push(format!(
            "recorded with save version {}, this build uses {SAVE_VERSION}",
            header.version
        ));
    }
    if header.ruleset != RULESET_VERSION {
        warnings.push(format!(
            "recorded under ruleset {}, this build uses {RULESET_VERSION}; the replay may diverge",
            header.ruleset
        ));
    }
    let current = hex(horde_core::catalog::content_fingerprint());
    if header.content_fingerprint != current {
        warnings.push(format!(
            "content fingerprint {} differs from this build's {current}; the replay may diverge",
            header.content_fingerprint
        ));
    }
    if header.seed != game.seed {
        warnings.push(format!(
            "header seed {} differs from the embedded game's seed {}",
            header.seed, game.seed
        ));
    }
    warnings
}

/// Replays `lines`. Catalogues must already be initialised.
pub fn replay(lines: Vec<Line>) -> Result<ReplayReport, String> {
    let mut iter = lines.into_iter();
    let Some(Line::Header(header)) = iter.next() else {
        return Err("recording does not start with a header line".into());
    };
    let mut game: Game = serde_json::from_value(header.initial.clone())
        .map_err(|e| format!("recorded initial state is unreadable: {e}"))?;
    let mut warnings = compatibility_warnings(&header, &game);
    let body: Vec<Line> = iter.collect();
    let (end_tick, recorded_digest) = match body.last() {
        Some(Line::End { tick, digest, .. }) => (Some(*tick), Some(digest.clone())),
        _ => (None, None),
    };
    let end_tick = end_tick.unwrap_or_else(|| {
        warnings
            .push("recording has no end line (unclean stop); replaying to the last input".into());
        body.iter()
            .filter_map(|line| match line {
                Line::Input { tick, .. } => Some(tick + 1),
                _ => None,
            })
            .max()
            .unwrap_or(header.start_tick)
    });
    let mut tick = header.start_tick;
    let mut input = Input::default();
    let mut commands = 0;
    let mut pending = body.into_iter().peekable();
    loop {
        while let Some(line) = pending.next_if(|line| match line {
            Line::Command { tick: t, .. } | Line::Input { tick: t, .. } => *t <= tick,
            Line::Header(_) => true,
            Line::End { .. } => false,
        }) {
            match line {
                Line::Command { command, .. } => {
                    commands += 1;
                    apply(&mut game, &command);
                }
                Line::Input { input: next, .. } => input = next,
                Line::Header(_) => return Err("recording has a second header line".into()),
                Line::End { .. } => {}
            }
        }
        if tick >= end_tick {
            break;
        }
        // Mirrors `Runtime::run_tick`: finishing the attempt pauses the game.
        let had_attempt = game.attempt.is_some();
        game.tick(input.clone());
        for _ in game.drain_events() {}
        let _ = game.take_settlement_issues();
        if had_attempt && game.attempt.is_none() {
            game.set_paused(true);
        }
        tick += 1;
    }
    Ok(ReplayReport {
        ticks: tick - header.start_tick,
        commands,
        digest: game.state_digest(),
        recorded_digest,
        warnings,
    })
}

/// Rejected commands were rejected when recorded too; the outcome is state, not an error.
fn apply(game: &mut Game, command: &Command) {
    let _ = game.apply(command);
    let _ = game.take_settlement_issues();
}
