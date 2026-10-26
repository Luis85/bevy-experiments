//! Orders input: right-click move, follow and Shift-queue, the rotate and follow modes, the stop
//! key, and the testbed hotkeys for pause, speed and the local player (spec §10, ADR-0008).
//!
//! Input becomes [`OrderInput`] messages; one system plans each into a [`PlannedOrder`] with the
//! pure [`order_for`] and issues it as one `PlayerCommand` for every selected unit, as the
//! [`LocalPlayer`], through [`LocalDriver`]. The client never mutates sim state. Picking and the
//! cursor ray need a window and a GPU, so the wiring is covered by the demo checklist; the logic
//! is in [`order_for`], [`ground_to_sim`], [`next_speed`] and [`next_player`].
use bevy::prelude::*;
use fl_math::{Fx, FxVec2};
use fl_protocol::{CommandPayload, FollowCmd, MoveCmd, PlayerId, RotateCmd, SimId, StopCmd};
use fl_view::{CommandDraft, TickDriver};

use crate::debug::CursorGround;
use crate::driver::LocalDriver;
use crate::link::{RunnerLink, SnapshotBuffer, encode_command};
use crate::placeholder::PlayerColors;
use crate::selection::{
    ApplySelectInput, ClickedUnit, FillOnScreen, InputGate, SelectInput, Selection,
    SelectionPlugin, mouse_input,
};
use crate::view::{LocalPlayer, SimViewSystems};

/// The most units one command may carry; the sim's `MAX_COMMAND_UNITS` (the client can't depend
/// on `fl_sim`), so a larger selection is split into consecutive commands.
pub const MAX_COMMAND_UNITS: usize = 256;

/// The game speeds the speed keys step through.
pub const SPEED_STEPS: [f32; 6] = [0.25, 0.5, 1.0, 2.0, 4.0, 8.0];

/// What a left click does next: nothing special, turn the selection toward a point, or have it
/// follow a unit.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderMode {
    #[default]
    Normal,
    Rotate,
    Follow,
}

/// One order gesture, as the input systems report it.
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub enum OrderInput {
    /// A right click on the ground and/or a unit.
    RightClick {
        ground: Option<Vec3>,
        unit: Option<SimId>,
        shift: bool,
        alt: bool,
    },
    /// A left click while [`OrderMode`] is not `Normal`.
    LeftClickInMode {
        ground: Option<Vec3>,
        unit: Option<SimId>,
        shift: bool,
    },
    /// The stop key.
    Stop,
}

/// An order ready to become a command.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PlannedOrder {
    Move { target: FxVec2, queue: bool },
    Follow { leader: SimId, queue: bool },
    Rotate { toward: FxVec2, queue: bool },
    Stop,
}

/// The order `input` gives `selection` in `mode`, or `None` when it gives none.
///
/// Right click: an own unit (without Alt) is followed; anything else moves to the ground point.
/// Left click in a mode: `Rotate` turns toward the ground point, `Follow` follows the clicked
/// unit, `Normal` does nothing. An empty selection never gets an order.
#[must_use]
pub fn order_for(
    input: &OrderInput,
    mode: OrderMode,
    selection: &Selection,
    is_own: impl Fn(SimId) -> bool,
) -> Option<PlannedOrder> {
    if selection.0.is_empty() {
        return None;
    }
    match *input {
        OrderInput::Stop => Some(PlannedOrder::Stop),
        OrderInput::RightClick {
            ground,
            unit,
            shift,
            alt,
        } => match unit {
            Some(leader) if !alt && is_own(leader) => Some(PlannedOrder::Follow {
                leader,
                queue: shift,
            }),
            _ => ground.map(|p| PlannedOrder::Move {
                target: ground_to_sim(p),
                queue: shift,
            }),
        },
        OrderInput::LeftClickInMode {
            ground,
            unit,
            shift,
        } => match mode {
            OrderMode::Normal => None,
            OrderMode::Rotate => ground.map(|p| PlannedOrder::Rotate {
                toward: ground_to_sim(p),
                queue: shift,
            }),
            OrderMode::Follow => unit.map(|leader| PlannedOrder::Follow {
                leader,
                queue: shift,
            }),
        },
    }
}

/// The sim position (metres, `x` east, `y` north) of a world point: its `(x, z)`.
///
/// A coordinate beyond the fixed-point range (or not finite) saturates, so a stray cursor ray
/// never yields a panic.
#[must_use]
pub fn ground_to_sim(p: Vec3) -> FxVec2 {
    let metres = |v: f32| match Fx::from_f64(f64::from(v)) {
        Ok(fx) => fx,
        Err(_) if v > 0.0 => Fx::from_raw(i64::MAX),
        Err(_) if v < 0.0 => Fx::from_raw(i64::MIN),
        Err(_) => Fx::ZERO,
    };
    FxVec2::new(metres(p.x), metres(p.z))
}

/// The keys of the testbed controls; M6's command grid will rebind them.
#[derive(Resource, Clone, Debug)]
pub struct TestbedKeys {
    pub pause: KeyCode,
    pub speed_up: KeyCode,
    pub speed_down: KeyCode,
    pub cycle_player: KeyCode,
    pub stop: KeyCode,
    pub rotate: KeyCode,
    pub follow: KeyCode,
}

impl Default for TestbedKeys {
    fn default() -> Self {
        TestbedKeys {
            pause: KeyCode::KeyP,
            speed_up: KeyCode::Equal,
            speed_down: KeyCode::Minus,
            cycle_player: KeyCode::F4,
            stop: KeyCode::KeyH,
            rotate: KeyCode::KeyR,
            follow: KeyCode::KeyF,
        }
    }
}

/// The step after `speed` in [`SPEED_STEPS`] (`up`) or before it, holding at either end.
#[must_use]
pub fn next_speed(speed: f32, up: bool) -> f32 {
    const EPS: f32 = 1e-3;
    if up {
        SPEED_STEPS
            .into_iter()
            .find(|&s| s > speed + EPS)
            .unwrap_or(SPEED_STEPS[SPEED_STEPS.len() - 1])
    } else {
        SPEED_STEPS
            .into_iter()
            .rev()
            .find(|&s| s < speed - EPS)
            .unwrap_or(SPEED_STEPS[0])
    }
}

/// The player after `current` in `players` (ascending), wrapping; the first when `current` is
/// not listed. `None` for an empty list.
#[must_use]
pub fn next_player(players: &[PlayerId], current: Option<PlayerId>) -> Option<PlayerId> {
    let first = *players.first()?;
    Some(
        current
            .and_then(|c| players.iter().copied().find(|&p| p > c))
            .unwrap_or(first),
    )
}

/// The systems of [`OrdersPlugin`] that turn input into [`OrderInput`] messages.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct OrderInputSystems;

/// The system that issues [`OrderInput`] messages as commands.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ApplyOrders;

/// Order input, the order modes and the testbed hotkeys.
pub struct OrdersPlugin;

impl Plugin for OrdersPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<SelectionPlugin>() {
            app.add_plugins(SelectionPlugin);
        }
        app.init_resource::<OrderMode>()
            .init_resource::<TestbedKeys>()
            .init_resource::<CursorGround>()
            .add_message::<OrderInput>()
            .add_systems(
                Update,
                (
                    // Between the selection's mouse system and its fill/apply systems, so a
                    // click in a mode is taken before it can clear the selection.
                    mouse_orders
                        .run_if(resource_exists::<ButtonInput<MouseButton>>)
                        .after(mouse_input)
                        .before(FillOnScreen),
                    key_orders
                        .run_if(resource_exists::<ButtonInput<KeyCode>>)
                        .after(SimViewSystems)
                        .before(FillOnScreen),
                    apply_orders
                        .in_set(ApplyOrders)
                        .after(ApplySelectInput)
                        .after(mouse_orders)
                        .after(key_orders),
                )
                    .in_set(OrderInputSystems),
            );
    }
}

fn shift_held(keys: Option<&ButtonInput<KeyCode>>) -> bool {
    keys.is_some_and(|k| k.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]))
}

/// Right clicks become [`OrderInput::RightClick`]. While a mode is on, the selection's left
/// clicks are taken out of the stream and become [`OrderInput::LeftClickInMode`].
#[expect(
    clippy::too_many_arguments,
    reason = "a Bevy system; each parameter is one resource"
)]
fn mouse_orders(
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Option<Res<ButtonInput<KeyCode>>>,
    gate: InputGate,
    cursor: Res<CursorGround>,
    clicked: Res<ClickedUnit>,
    mode: Res<OrderMode>,
    mut selects: ResMut<Messages<SelectInput>>,
    mut out: MessageWriter<OrderInput>,
) {
    if *mode != OrderMode::Normal {
        let taken: Vec<SelectInput> = selects.drain().collect();
        for input in taken {
            match input {
                SelectInput::Click { sim_id, shift, .. } => {
                    out.write(OrderInput::LeftClickInMode {
                        ground: cursor.0,
                        unit: sim_id,
                        shift,
                    });
                }
                other => {
                    selects.write(other);
                }
            }
        }
    }
    if mouse.just_released(MouseButton::Right) && !gate.pointer_blocked() {
        let keys = keys.as_deref();
        out.write(OrderInput::RightClick {
            ground: cursor.0,
            unit: clicked.secondary,
            shift: shift_held(keys),
            alt: keys.is_some_and(|k| k.any_pressed([KeyCode::AltLeft, KeyCode::AltRight])),
        });
    }
}

/// The stop, mode, pause, speed and player keys.
#[expect(
    clippy::too_many_arguments,
    reason = "a Bevy system; each parameter is one resource"
)]
fn key_orders(
    keys: Res<ButtonInput<KeyCode>>,
    bindings: Res<TestbedKeys>,
    gate: InputGate,
    mut mode: ResMut<OrderMode>,
    mut selection: ResMut<Selection>,
    colors: Option<Res<PlayerColors>>,
    player: Option<Res<LocalPlayer>>,
    driver: Option<ResMut<LocalDriver>>,
    mut commands: Commands,
    mut out: MessageWriter<OrderInput>,
) {
    if gate.keyboard_blocked() {
        return;
    }
    if keys.just_pressed(bindings.stop) {
        out.write(OrderInput::Stop);
    }
    for (key, target) in [
        (bindings.rotate, OrderMode::Rotate),
        (bindings.follow, OrderMode::Follow),
    ] {
        if keys.just_pressed(key) {
            *mode = if *mode == target {
                OrderMode::Normal
            } else {
                target
            };
        }
    }
    if let Some(mut driver) = driver {
        if keys.just_pressed(bindings.pause) {
            if driver.is_paused() {
                driver.resume();
            } else {
                driver.pause();
            }
        }
        for (key, up) in [(bindings.speed_up, true), (bindings.speed_down, false)] {
            if keys.just_pressed(key) {
                let speed = next_speed(driver.speed(), up);
                if speed != driver.speed() {
                    driver.set_speed(speed);
                }
            }
        }
    }
    if keys.just_pressed(bindings.cycle_player) {
        let players: Vec<PlayerId> = colors.iter().flat_map(|c| c.0.keys().copied()).collect();
        if let Some(next) = next_player(&players, player.as_deref().map(|p| p.0)) {
            commands.insert_resource(LocalPlayer(next));
            selection.0.clear();
            *mode = OrderMode::Normal;
        }
    }
}

/// Everything [`apply_orders`] needs to turn a plan into a command.
struct Issuer<'a> {
    link: &'a RunnerLink,
    player: PlayerId,
    units: Vec<SimId>,
}

impl Issuer<'_> {
    /// Issues `payload` for the units, in consecutive commands of at most
    /// [`MAX_COMMAND_UNITS`] each (the sim rejects a longer list), all with the same `queue`.
    fn issue<P: CommandPayload>(&self, driver: &mut LocalDriver, payload: &P, queue: bool) {
        let kind = match encode_command(&self.link.header.string_tables, payload) {
            Ok(kind) => kind,
            Err(err) => {
                tracing::warn!(%err, command = P::NAME, "order not issued");
                return;
            }
        };
        for units in self.units.chunks(MAX_COMMAND_UNITS) {
            driver.issue(CommandDraft {
                player: self.player,
                units: units.to_vec(),
                kind: kind.clone(),
                queue,
            });
        }
    }
}

/// Plans each [`OrderInput`] and issues it for the whole selection. Any order that results,
/// whatever its source, ends [`OrderMode`] (back to `Normal`) unless it is queued with Shift.
fn apply_orders(
    mut inputs: MessageReader<OrderInput>,
    selection: Res<Selection>,
    mut mode: ResMut<OrderMode>,
    buffer: Res<SnapshotBuffer>,
    link: Option<Res<RunnerLink>>,
    driver: Option<ResMut<LocalDriver>>,
    player: Option<Res<LocalPlayer>>,
) {
    let (Some(link), Some(mut driver)) = (link, driver) else {
        inputs.clear();
        return;
    };
    let Some(LocalPlayer(player)) = player.as_deref().copied() else {
        if inputs.read().next().is_some() {
            tracing::warn!("order not issued: there is no local player");
        }
        return;
    };
    let snapshot = buffer.curr.as_deref();
    for input in inputs.read() {
        let mut input = *input;
        // A unit hit with no ground hit stands in for the ground at its own position.
        if let OrderInput::RightClick {
            ground: ground @ None,
            unit: Some(id),
            ..
        } = &mut input
            && let Some(record) = snapshot.and_then(|s| s.get(*id))
        {
            *ground = Some(Vec3::new(record.pos.x.to_f32(), 0.0, record.pos.y.to_f32()));
        }
        let is_own = |id: SimId| {
            snapshot
                .and_then(|s| s.get(id))
                .is_some_and(|r| r.owner == player)
        };
        let Some(plan) = order_for(&input, *mode, &selection, is_own) else {
            continue;
        };
        let mut issuer = Issuer {
            link: &link,
            player,
            units: selection.0.iter().copied().collect(),
        };
        let shift = match plan {
            PlannedOrder::Move { target, queue } => {
                issuer.issue(&mut driver, &MoveCmd { target }, queue);
                queue
            }
            PlannedOrder::Rotate { toward, queue } => {
                issuer.issue(&mut driver, &RotateCmd { toward }, queue);
                queue
            }
            PlannedOrder::Follow { leader, queue } => {
                // A unit can't follow itself.
                issuer.units.retain(|&id| id != leader);
                if !issuer.units.is_empty() {
                    issuer.issue(&mut driver, &FollowCmd { leader }, queue);
                }
                queue
            }
            PlannedOrder::Stop => {
                issuer.issue(&mut driver, &StopCmd, false);
                false
            }
        };
        if !shift && *mode != OrderMode::Normal {
            *mode = OrderMode::Normal;
        }
    }
}
