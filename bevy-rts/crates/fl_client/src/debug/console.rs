//! The debug console: a registry of named commands and the built-in ones (spec §11).
//!
//! Every sim change a console command makes is a `debug.*` command issued through
//! [`LocalDriver`] (ADR-0008). Pause, step and speed are driver controls; `player` and `reveal`
//! are client-local render overrides; `inspect` sends a [`SimQuery`](fl_view::SimQuery).
use std::collections::BTreeMap;
use std::str::FromStr;

use bevy::prelude::*;
use bevy_egui::{EguiContext, PrimaryEguiContext, egui};
use fl_math::{Angle, Fx, FxVec2};
use fl_protocol::{
    CommandPayload, DebugInvulnerableCmd, DebugSetHpCmd, DebugSetOwnerCmd, DebugSetTerrainCmd,
    DebugSpawnCmd, DebugTeleportCmd, DebugWanderCmd, PlayerCommand, PlayerId, SimId,
};
use fl_view::{CommandDraft, TickDriver};

use crate::debug::inspector::inspect;
use crate::driver::{LocalDriver, MAX_SPEED, MIN_SPEED};
use crate::link::{RunnerLink, RunnerStatus, SnapshotBuffer, encode_command};
use crate::view::{ClientContent, LocalPlayer, RevealAll};

/// A console command: gets the world and the arguments after its name; returns the text to show.
pub type ConsoleFn = fn(&mut World, &[&str]) -> Result<String, String>;

/// The console commands, by name.
#[derive(Resource, Default)]
pub struct ConsoleCommands {
    commands: BTreeMap<&'static str, (&'static str, ConsoleFn)>,
}

impl ConsoleCommands {
    /// Adds (or replaces) the command `name`; `help` is its one-line usage.
    pub fn insert(&mut self, name: &'static str, help: &'static str, f: ConsoleFn) {
        self.commands.insert(name, (help, f));
    }

    /// The command `name`.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<ConsoleFn> {
        self.commands.get(name).map(|&(_, f)| f)
    }

    /// Every command's name and help, by name.
    pub fn iter(&self) -> impl Iterator<Item = (&'static str, &'static str)> + '_ {
        self.commands.iter().map(|(&name, &(help, _))| (name, help))
    }
}

/// Registers console commands on an [`App`].
pub trait ConsoleAppExt {
    /// Adds the console command `name` with one-line usage `help`.
    fn register_console_command(
        &mut self,
        name: &'static str,
        help: &'static str,
        f: ConsoleFn,
    ) -> &mut Self;
}

impl ConsoleAppExt for App {
    fn register_console_command(
        &mut self,
        name: &'static str,
        help: &'static str,
        f: ConsoleFn,
    ) -> &mut Self {
        self.world_mut()
            .get_resource_or_init::<ConsoleCommands>()
            .insert(name, help, f);
        self
    }
}

/// The console window: the line being typed, what was shown so far, and whether it is open.
#[derive(Resource, Default)]
pub struct ConsoleState {
    pub input: String,
    pub history: Vec<String>,
    pub open: bool,
    /// Whether the toggle key was pressed this frame.
    toggled: bool,
}

/// Runs one console line: splits it on whitespace and calls the command its first word names.
///
/// Returns the command's output, `"error: …"` when it fails, `"unknown command: <name>"` when no
/// command has that name, and an empty string for a blank line.
pub fn run_console_line(world: &mut World, line: &str) -> String {
    let mut words = line.split_whitespace();
    let Some(name) = words.next() else {
        return String::new();
    };
    let args: Vec<&str> = words.collect();
    let command = world
        .get_resource::<ConsoleCommands>()
        .and_then(|commands| commands.get(name));
    match command {
        None => format!("unknown command: {name}"),
        Some(f) => f(world, &args).unwrap_or_else(|err| format!("error: {err}")),
    }
}

/// Stamps `payload` as a command of `player` for `units` and queues it in `driver`.
///
/// # Errors
///
/// If the payload is `debug.*` and the match disallows cheats, or it fails to encode.
pub fn issue_command<P: CommandPayload>(
    link: &RunnerLink,
    driver: &mut LocalDriver,
    player: PlayerId,
    units: Vec<SimId>,
    payload: &P,
) -> Result<PlayerCommand, String> {
    if P::DEBUG_ONLY && !link.header.cheats_allowed {
        return Err(format!(
            "'{}' needs cheats; this match disallows them",
            P::NAME
        ));
    }
    let kind = encode_command(&link.header.string_tables, payload)?;
    Ok(driver.issue(CommandDraft {
        player,
        units,
        kind,
        queue: false,
    }))
}

/// [`issue_command`] as the [`LocalPlayer`], with the [`RunnerLink`] and [`LocalDriver`] of
/// `world`. Every `debug.*` command is issued as the local player (ruling R44); the sim skips
/// its ownership filter for debug kinds when cheats are allowed.
///
/// # Errors
///
/// If no match is running, there is no local player, or [`issue_command`] fails.
pub fn issue_in_world<P: CommandPayload>(
    world: &mut World,
    units: Vec<SimId>,
    payload: &P,
) -> Result<PlayerCommand, String> {
    if !world.contains_resource::<RunnerLink>() || !world.contains_resource::<LocalDriver>() {
        return Err(NO_MATCH.to_owned());
    }
    let LocalPlayer(player) = *world
        .get_resource::<LocalPlayer>()
        .ok_or("there is no local player; run 'player <id>' first")?;
    world.resource_scope(|world, mut driver: Mut<LocalDriver>| {
        let link = world.resource::<RunnerLink>();
        issue_command(link, &mut driver, player, units, payload)
    })
}

const NO_MATCH: &str = "no match is running";

/// Adds the built-in console commands.
pub(crate) fn register_builtin_commands(app: &mut App) {
    app.register_console_command("help", "help: list the commands", help)
        .register_console_command(
            "spawn",
            "spawn <unit_id> <player> <x> <y>: debug.spawn a unit owned by <player>",
            spawn,
        )
        .register_console_command(
            "wander",
            "wander <player> <radius>: debug.wander the player's units",
            wander,
        )
        .register_console_command(
            "setowner",
            "setowner <sim_id> <player>: debug.set_owner",
            set_owner,
        )
        .register_console_command(
            "invuln",
            "invuln <sim_id> on|off: debug.invulnerable",
            invulnerable,
        )
        .register_console_command("sethp", "sethp <sim_id> <hp>: debug.set_hp", set_hp)
        .register_console_command(
            "teleport",
            "teleport <sim_id> <x> <y>: debug.teleport",
            teleport,
        )
        .register_console_command(
            "terrain",
            "terrain <x0> <y0> <x1> <y1> <id>: debug.set_terrain the cells (inclusive) to <id>",
            terrain,
        )
        .register_console_command(
            "player",
            "player <id>: view and command as that player",
            player,
        )
        .register_console_command("reveal", "reveal: toggle drawing every unit", reveal)
        .register_console_command("pause", "pause: stop ticking", pause)
        .register_console_command("resume", "resume: tick again", resume)
        .register_console_command("step", "step [n]: while paused, run n ticks (1)", step)
        .register_console_command("speed", "speed <0.25..8>: set the game speed", speed)
        .register_console_command("hash", "hash: the last state hash", hash)
        .register_console_command("tick", "tick: the current tick", tick)
        .register_console_command(
            "inspect",
            "inspect <sim_id>: query a unit into the inspector",
            inspect_command,
        );
}

/// Checks the argument count against a command's usage.
fn expect_args<'a, const N: usize>(args: &[&'a str], usage: &str) -> Result<[&'a str; N], String> {
    <[&str; N]>::try_from(args).map_err(|_| format!("usage: {usage}"))
}

fn parse<T: FromStr>(arg: &str, what: &str) -> Result<T, String> {
    arg.parse()
        .map_err(|_| format!("'{arg}' is not a valid {what}"))
}

fn parse_player(arg: &str) -> Result<PlayerId, String> {
    parse(arg, "player id").map(PlayerId)
}

fn parse_sim_id(arg: &str) -> Result<SimId, String> {
    parse(arg, "sim id").map(SimId)
}

/// A distance or coordinate in metres, as fixed point.
fn parse_metres(arg: &str) -> Result<Fx, String> {
    let value: f64 = parse(arg, "number")?;
    if !value.is_finite() {
        return Err(format!("'{arg}' is not a valid number"));
    }
    Fx::from_f64(value).map_err(|err| err.to_string())
}

fn parse_pos(x: &str, y: &str) -> Result<FxVec2, String> {
    Ok(FxVec2::new(parse_metres(x)?, parse_metres(y)?))
}

/// Issues `payload` for the one unit `sim_id`, which may be any unit (ruling R44).
fn issue_for_unit<P: CommandPayload>(
    world: &mut World,
    sim_id: SimId,
    payload: &P,
) -> Result<String, String> {
    let command = issue_in_world(world, vec![sim_id], payload)?;
    Ok(issued(&command, P::NAME))
}

fn issued(command: &PlayerCommand, name: &str) -> String {
    format!(
        "{name} issued for tick {} (player {}, seq {})",
        command.exec_tick.0, command.player.0, command.seq
    )
}

fn help(world: &mut World, _args: &[&str]) -> Result<String, String> {
    let commands = world.get_resource::<ConsoleCommands>();
    Ok(commands
        .map(|c| {
            c.iter()
                .map(|(_, help)| help)
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default())
}

fn spawn(world: &mut World, args: &[&str]) -> Result<String, String> {
    let [unit, owner, x, y] = expect_args(args, "spawn <unit_id> <player> <x> <y>")?;
    let owner = parse_player(owner)?;
    let pos = parse_pos(x, y)?;
    if let Some(content) = world.get_resource::<ClientContent>()
        && content.0.unit_id(unit).is_none()
    {
        return Err(format!("no unit named '{unit}'"));
    }
    let payload = DebugSpawnCmd {
        unit: unit.to_owned(),
        owner,
        pos,
        heading: Angle::ZERO,
    };
    let command = issue_in_world(world, Vec::new(), &payload)?;
    Ok(issued(&command, DebugSpawnCmd::NAME))
}

fn wander(world: &mut World, args: &[&str]) -> Result<String, String> {
    let [player, radius] = expect_args(args, "wander <player> <radius>")?;
    let player = parse_player(player)?;
    let radius = parse_metres(radius)?;
    if radius < Fx::ZERO {
        return Err("the radius must not be negative".to_owned());
    }
    let units: Vec<SimId> = world
        .get_resource::<SnapshotBuffer>()
        .and_then(|buffer| buffer.curr.as_deref())
        .map(|snapshot| {
            snapshot
                .entities
                .iter()
                .filter(|record| record.owner == player)
                .map(|record| record.sim_id)
                .collect()
        })
        .unwrap_or_default();
    if units.is_empty() {
        return Err(format!(
            "player {} has no units in the current snapshot",
            player.0
        ));
    }
    let count = units.len();
    let command = issue_in_world(world, units, &DebugWanderCmd { radius })?;
    Ok(format!(
        "{} ({count} units)",
        issued(&command, DebugWanderCmd::NAME)
    ))
}

fn set_owner(world: &mut World, args: &[&str]) -> Result<String, String> {
    let [sim_id, owner] = expect_args(args, "setowner <sim_id> <player>")?;
    let (sim_id, owner) = (parse_sim_id(sim_id)?, parse_player(owner)?);
    issue_for_unit(world, sim_id, &DebugSetOwnerCmd { owner })
}

fn invulnerable(world: &mut World, args: &[&str]) -> Result<String, String> {
    let [sim_id, flag] = expect_args(args, "invuln <sim_id> on|off")?;
    let sim_id = parse_sim_id(sim_id)?;
    let on = match flag {
        "on" => true,
        "off" => false,
        other => return Err(format!("'{other}' is neither 'on' nor 'off'")),
    };
    issue_for_unit(world, sim_id, &DebugInvulnerableCmd { on })
}

fn set_hp(world: &mut World, args: &[&str]) -> Result<String, String> {
    let [sim_id, hp] = expect_args(args, "sethp <sim_id> <hp>")?;
    let (sim_id, hp) = (parse_sim_id(sim_id)?, parse(hp, "hit point count")?);
    issue_for_unit(world, sim_id, &DebugSetHpCmd { hp })
}

fn teleport(world: &mut World, args: &[&str]) -> Result<String, String> {
    let [sim_id, x, y] = expect_args(args, "teleport <sim_id> <x> <y>")?;
    let (sim_id, pos) = (parse_sim_id(sim_id)?, parse_pos(x, y)?);
    issue_for_unit(world, sim_id, &DebugTeleportCmd { pos })
}

fn terrain(world: &mut World, args: &[&str]) -> Result<String, String> {
    let [x0, y0, x1, y1, id] = expect_args(args, "terrain <x0> <y0> <x1> <y1> <id>")?;
    let (x0, y0): (u32, u32) = (parse(x0, "cell index")?, parse(y0, "cell index")?);
    let (x1, y1): (u32, u32) = (parse(x1, "cell index")?, parse(y1, "cell index")?);
    let payload = DebugSetTerrainCmd {
        min_cell: (x0.min(x1), y0.min(y1)),
        max_cell: (x0.max(x1), y0.max(y1)),
        terrain: id.to_owned(),
    };
    let command = issue_in_world(world, Vec::new(), &payload)?;
    Ok(issued(&command, DebugSetTerrainCmd::NAME))
}

fn player(world: &mut World, args: &[&str]) -> Result<String, String> {
    let [id] = expect_args(args, "player <id>")?;
    let player = parse_player(id)?;
    world.insert_resource(LocalPlayer(player));
    Ok(format!("local player is now {}", player.0))
}

fn reveal(world: &mut World, _args: &[&str]) -> Result<String, String> {
    let mut reveal = world.get_resource_or_init::<RevealAll>();
    reveal.0 = !reveal.0;
    Ok(format!(
        "reveal all {}",
        if reveal.0 { "on" } else { "off" }
    ))
}

fn driver_mut(world: &mut World) -> Result<Mut<'_, LocalDriver>, String> {
    world
        .get_resource_mut::<LocalDriver>()
        .ok_or_else(|| NO_MATCH.to_owned())
}

fn pause(world: &mut World, _args: &[&str]) -> Result<String, String> {
    driver_mut(world)?.pause();
    Ok("paused".to_owned())
}

fn resume(world: &mut World, _args: &[&str]) -> Result<String, String> {
    driver_mut(world)?.resume();
    Ok("resumed".to_owned())
}

fn step(world: &mut World, args: &[&str]) -> Result<String, String> {
    let n = match args {
        [] => 1,
        [n] => parse(n, "tick count")?,
        _ => return Err("usage: step [n]".to_owned()),
    };
    let mut driver = driver_mut(world)?;
    if !driver.is_paused() {
        return Err("step needs a paused game; run 'pause' first".to_owned());
    }
    driver.step(n);
    Ok(format!("stepping {n} tick(s)"))
}

fn speed(world: &mut World, args: &[&str]) -> Result<String, String> {
    let [x] = expect_args(args, "speed <0.25..8>")?;
    let x: f32 = parse(x, "speed")?;
    if !(MIN_SPEED..=MAX_SPEED).contains(&x) {
        return Err(format!("the speed must be in {MIN_SPEED}..={MAX_SPEED}"));
    }
    driver_mut(world)?.set_speed(x);
    Ok(format!("speed x{x}"))
}

fn hash(world: &mut World, _args: &[&str]) -> Result<String, String> {
    let status = world.get_resource::<RunnerStatus>();
    Ok(match status.and_then(|s| s.last_hash) {
        Some(hash) => format!("{hash:016x}"),
        None => "no state hash received yet".to_owned(),
    })
}

fn tick(world: &mut World, _args: &[&str]) -> Result<String, String> {
    let last = world
        .get_resource::<RunnerStatus>()
        .and_then(|s| s.last_tick);
    let next = world
        .get_resource::<LocalDriver>()
        .map(TickDriver::next_tick);
    Ok(match (last, next) {
        (Some(last), Some(next)) => format!("tick {} (next sent {})", last.0, next.0),
        (Some(last), None) => format!("tick {}", last.0),
        (None, _) => "no tick received yet".to_owned(),
    })
}

fn inspect_command(world: &mut World, args: &[&str]) -> Result<String, String> {
    let [sim_id] = expect_args(args, "inspect <sim_id>")?;
    let sim_id = parse_sim_id(sim_id)?;
    let id = inspect(world, sim_id)?;
    Ok(format!("inspecting unit {sim_id} (query {id})"))
}

/// Opens or closes the console on backquote or F1.
pub(crate) fn toggle_console(keys: Res<ButtonInput<KeyCode>>, mut state: ResMut<ConsoleState>) {
    let toggled = keys.any_just_pressed([KeyCode::Backquote, KeyCode::F1]);
    if toggled {
        state.open = !state.open;
    }
    if state.toggled != toggled {
        state.toggled = toggled;
    }
}

/// How many lines of output the console keeps.
const HISTORY_LINES: usize = 500;

/// Draws the console window and runs the line submitted with Enter.
pub(crate) fn console_window(world: &mut World) {
    let mut contexts = world.query_filtered::<&mut EguiContext, With<PrimaryEguiContext>>();
    let Ok(mut context) = contexts.single_mut(world) else {
        return;
    };
    let ctx = context.get_mut().clone();
    let submitted = world.resource_scope(|_, mut state: Mut<ConsoleState>| {
        let state = &mut *state;
        if state.toggled {
            // Consume the toggle key's text before the line sees it.
            ctx.input_mut(|input| {
                input
                    .events
                    .retain(|event| !matches!(event, egui::Event::Text(text) if text == "`"));
            });
        }
        let mut open = state.open;
        let mut submitted = None;
        egui::Window::new("Console")
            .open(&mut open)
            .default_width(520.0)
            .show(&ctx, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(260.0)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for line in &state.history {
                            ui.monospace(line);
                        }
                    });
                let response = ui.add(
                    egui::TextEdit::singleline(&mut state.input)
                        .desired_width(f32::INFINITY)
                        .hint_text("help"),
                );
                if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    submitted = Some(std::mem::take(&mut state.input));
                    response.request_focus();
                }
            });
        state.open = open;
        submitted
    });
    let Some(line) = submitted else {
        return;
    };
    let output = run_console_line(world, &line);
    let mut state = world.resource_mut::<ConsoleState>();
    state.history.push(format!("> {line}"));
    state.history.extend(output.lines().map(str::to_owned));
    let excess = state.history.len().saturating_sub(HISTORY_LINES);
    state.history.drain(..excess);
}
