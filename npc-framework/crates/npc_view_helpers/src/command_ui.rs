//! RTS command UI for `npc_command` (feature `command`). View-only: it
//! keeps a multi-selection and control groups as view resources and turns
//! clicks into recorded `SimCommand`s (`IssueOrder`), so a host can swap in
//! its own UI and replays see exactly the orders that were given.
//!
//! Opt-in: add [`CommandUiPlugin`] (it is not part of
//! `NpcViewHelpersPlugin`). Bindings are data ([`CommandBindings`]), so the
//! app decides the keys; the defaults are the usual RTS ones:
//!
//! - left click: select one own unit (Shift: add/remove); left drag: box
//!   select (Shift: add);
//! - right click: an order for the selection (Shift: queue) of the kind
//!   [`RightClickMode`] says (the host sets it from its own hotkeys; default
//!   `Move`):
//!   - **`Move`**: the context order. Ground → `Move`; another unit → the
//!     first [`ContextOrders`] resolver that answers (glue registers
//!     `pick_up`, `open`, `Mount`, ..), else `Follow` for a commandable unit,
//!     else `Move` to it;
//!   - **`AttackMove`** to the ground (or the clicked unit's position);
//!   - **`Patrol`** collects points in [`PatrolDraft`]: Shift+click adds one,
//!     a plain click adds the last one and issues `Patrol` (so does leaving
//!     the mode with points collected);
//!   - **`Custom(name)`**: the [`RightClickOrders`] resolver `name`;
//! - Ctrl+1–9 stores the selection as a control group, 1–9 recalls it;
//! - a group `Move` / `AttackMove` carries [`CommandFormation`] when the
//!   host set one (its formation hotkeys), else the sim's default;
//! - gizmos: selection rings, each selected unit's order chain (current
//!   order, then queued ones), the patrol draft and the drag box
//!   ([`DragBox`]) on the ground.
//!
//! The pure parts ([`click_select`], [`box_select`], [`store_group`],
//! [`recall_group`], [`context_order`], [`right_click_order`],
//! [`order_command`]) are what the systems call; hosts with their own input
//! can call them directly, plus the world helpers [`pick_unit`] (the unit
//! under a ray, with the same picker as clicks: `crate::pick`),
//! [`select_units`] / [`select_in_rect`] (scripted selection; set
//! [`DragBox`] to show a marquee), [`unit_screen_positions`] and
//! [`issue_to_selection`] / [`issue_to_units`]. A host system that sets
//! [`RightClickMode`] in the frame of the click runs
//! `.before(NpcCommandUiSet)`.
//!
//! **Registering resolvers** ([`CommandUiAppExt`], any plugin order, with
//! or without [`CommandUiPlugin`] yet): `app.add_right_click_order(name,
//! f)` / `app.add_context_order(name, f)`, or their `_subset` variants
//! whose resolvers also choose **which selected units** take the order
//! ([`ClickOrder`]: e.g. only the engineers of a mixed selection repair a
//! wall). The UI issues right-click orders with `skip_invalid`
//! ([`SkipInvalidOrders`], default on), so a unit an executor refuses is
//! skipped, not the whole order.
//!
//! **Order targets that are not units**: put [`PickTarget`] on the visual
//! root (with `VisualOf`) of a wall, a building or a resource: right clicks
//! pick it (a resolver gets it as [`RightClick::target`]), selection never
//! does.

use bevy::color::palettes::css;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use npc_command::{
    Commandable, Formation, IssueOrder, IssueReceipt, OrderQueue, Owner, PlayerId, UnitOrder,
    submit_order_report,
};
use npc_kernel::commands::CommandError;
use npc_kernel::{SimCommand, SimId, SimIndex, SimPos};

use crate::select::{Selectable, Selected, pick, stale};
use crate::visuals::{ViewHidden, hidden};

/// Marker on a visual root (with [`VisualOf`]) whose sim entity is a
/// right-click target ([`pick_unit`]) but never selected (clicks and boxes
/// select only [`Selectable`] visuals): walls, buildings, resources.
/// (Named so it does not clash with `bevy::picking::Pickable`.)
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct PickTarget;
use crate::visuals::{VisualIndex, VisualOf};
use crate::{ViewSystems, sim_to_world};

/// Which player the local UI commands, and whether it is active.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandUi {
    pub player: PlayerId,
    pub enabled: bool,
    pub gizmos: bool,
}

impl Default for CommandUi {
    fn default() -> Self {
        Self {
            player: PlayerId(0),
            enabled: true,
            gizmos: true,
        }
    }
}

/// The bindings the UI reacts to (insert your own before the plugin).
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct CommandBindings {
    pub select: MouseButton,
    pub order: MouseButton,
    /// Any of these held: add to the selection / queue the order.
    pub add: Vec<KeyCode>,
    /// Any of these held with a group key: store the group.
    pub store_group: Vec<KeyCode>,
    /// Control group keys, group 1 first.
    pub groups: [KeyCode; 9],
    /// A left press that moves less than this (px) is a click, not a box.
    pub drag_px: f32,
}

impl Default for CommandBindings {
    fn default() -> Self {
        Self {
            select: MouseButton::Left,
            order: MouseButton::Right,
            add: vec![KeyCode::ShiftLeft, KeyCode::ShiftRight],
            store_group: vec![KeyCode::ControlLeft, KeyCode::ControlRight],
            groups: [
                KeyCode::Digit1,
                KeyCode::Digit2,
                KeyCode::Digit3,
                KeyCode::Digit4,
                KeyCode::Digit5,
                KeyCode::Digit6,
                KeyCode::Digit7,
                KeyCode::Digit8,
                KeyCode::Digit9,
            ],
            drag_px: 6.0,
        }
    }
}

/// The selected units, sorted by `SimId`. `Selected` (journal panel,
/// overlays) follows its first unit.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct CommandSelection {
    pub units: Vec<SimId>,
}

/// The formation the UI gives group moves (`None`: the sim's
/// `CommandConfig::default_formation`). The host binds keys to it.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CommandFormation(pub Option<Formation>);

/// Control groups 1–9 (index 0 = group 1), each sorted by `SimId`.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct ControlGroups {
    pub groups: [Vec<SimId>; 9],
}

/// Decides the order for a right click on `target` (a sim entity) by
/// `player`; `None` passes to the next resolver.
pub type ContextOrderFn = fn(&World, PlayerId, Entity) -> Option<UnitOrder>;

/// An order a resolver chose, and for which units: `units: None` = the
/// whole selection; `Some(subset)` = just those (intersected with the
/// selection; an empty result gives no order).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClickOrder {
    pub units: Option<Vec<SimId>>,
    pub order: UnitOrder,
}

impl ClickOrder {
    /// `order` for the whole selection.
    pub fn all(order: UnitOrder) -> Self {
        Self { units: None, order }
    }

    /// `order` for `units` (of the selection).
    pub fn only(units: impl IntoIterator<Item = SimId>, order: UnitOrder) -> Self {
        Self {
            units: Some(units.into_iter().collect()),
            order,
        }
    }

    /// The units that take it, of `selection` (sorted by `SimId`).
    pub fn units_of(&self, selection: &[SimId]) -> Vec<SimId> {
        match &self.units {
            None => selection.to_vec(),
            Some(subset) => selection
                .iter()
                .copied()
                .filter(|id| subset.contains(id))
                .collect(),
        }
    }
}

impl From<UnitOrder> for ClickOrder {
    fn from(order: UnitOrder) -> Self {
        Self::all(order)
    }
}

/// Whether right-click orders skip the selected units that cannot take
/// them (`IssueOrder::skip_invalid`, logged) instead of being rejected
/// whole. Default `true`; insert `SkipInvalidOrders(false)` for the old
/// all-or-nothing behaviour. Scripted orders ([`issue_to_selection`]) are
/// all-or-nothing either way.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SkipInvalidOrders(pub bool);

impl Default for SkipInvalidOrders {
    fn default() -> Self {
        Self(true)
    }
}

/// A context resolver that also picks the units ([`ClickOrder`]): it sees
/// the whole click (player, selection, ground) and the target entity.
pub type ContextOrderSubsetFn = fn(&World, &RightClick, Entity) -> Option<ClickOrder>;

/// Right-click resolvers, sorted by name; the first that answers wins
/// (plain [`ContextOrderFn`]s and [`ContextOrderSubsetFn`]s together, in
/// name order).
#[derive(Resource, Clone, Default)]
pub struct ContextOrders {
    pub resolvers: Vec<(&'static str, ContextOrderFn)>,
    /// Resolvers that choose the units too.
    pub subset_resolvers: Vec<(&'static str, ContextOrderSubsetFn)>,
}

fn add_sorted<F>(list: &mut Vec<(&'static str, F)>, name: &'static str, f: F) {
    match list.binary_search_by_key(&name, |(n, _)| n) {
        Ok(i) => list[i].1 = f,
        Err(i) => list.insert(i, (name, f)),
    }
}

impl ContextOrders {
    /// Adds (or replaces) resolver `name`.
    pub fn add(&mut self, name: &'static str, f: ContextOrderFn) {
        self.subset_resolvers.retain(|(n, _)| *n != name);
        add_sorted(&mut self.resolvers, name, f);
    }

    /// Adds (or replaces) subset resolver `name`.
    pub fn add_subset(&mut self, name: &'static str, f: ContextOrderSubsetFn) {
        self.resolvers.retain(|(n, _)| *n != name);
        add_sorted(&mut self.subset_resolvers, name, f);
    }

    /// The first resolver (by name) that answers a click on `target`.
    pub fn resolve(&self, world: &World, click: &RightClick, target: Entity) -> Option<ClickOrder> {
        let (mut a, mut b) = (
            self.resolvers.iter().peekable(),
            self.subset_resolvers.iter().peekable(),
        );
        loop {
            let plain_first = match (a.peek(), b.peek()) {
                (None, None) => return None,
                (Some(_), None) => true,
                (None, Some(_)) => false,
                (Some((na, _)), Some((nb, _))) => na <= nb,
            };
            let found = if plain_first {
                let (_, f) = a.next().expect("peeked");
                f(world, click.player, target).map(ClickOrder::all)
            } else {
                let (_, f) = b.next().expect("peeked");
                f(world, click, target)
            };
            if found.is_some() {
                return found;
            }
        }
    }
}

/// The kind of order a right click gives (see [`RightClickMode`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum RightClickKind {
    /// The context order ([`context_order`]).
    #[default]
    Move,
    /// `AttackMove` to the ground point (or the clicked unit's position).
    AttackMove,
    /// `Patrol` through the points collected in [`PatrolDraft`].
    Patrol,
    /// The [`RightClickOrders`] resolver registered under this name.
    Custom(&'static str),
}

/// What the next right click orders. The host sets it from its own hotkeys
/// (the library binds none): every frame while a key is held
/// ([`RightClickMode::held`]), or for one order ([`RightClickMode::once`]:
/// press A, then click), after which the plugin puts it back to `Move`.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RightClickMode {
    pub kind: RightClickKind,
    /// Back to [`RightClickKind::Move`] once an order of this kind went out.
    pub once: bool,
}

impl RightClickMode {
    /// `kind` until the host changes it.
    pub const fn held(kind: RightClickKind) -> Self {
        Self { kind, once: false }
    }

    /// `kind` for one order, then `Move`.
    pub const fn once(kind: RightClickKind) -> Self {
        Self { kind, once: true }
    }
}

/// The patrol points (`(x, z)` mm) collected while the mode is
/// [`RightClickKind::Patrol`]: Shift+right click adds a point, a plain right
/// click adds the last one and issues `Patrol { points }` for the selection;
/// leaving the mode with points collected issues them too.
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)]
pub struct PatrolDraft {
    pub points: Vec<(i32, i32)>,
}

/// One right click, as order resolvers see it.
#[derive(Clone, Copy, Debug)]
pub struct RightClick<'a> {
    pub player: PlayerId,
    /// The units the order is for (sorted by `SimId`).
    pub selection: &'a [SimId],
    /// The sim entity under the cursor ([`pick_unit`]), if any.
    pub target: Option<Entity>,
    /// Where the cursor ray hits the ground, if it does.
    pub ground: Option<SimPos>,
}

/// A custom right-click order kind ([`RightClickKind::Custom`]); `None`: no
/// order for this click.
pub type RightClickOrderFn = fn(&World, &RightClick) -> Option<UnitOrder>;

/// A custom right-click order kind that also picks the units of the
/// selection that take it ([`ClickOrder`]).
pub type RightClickSubsetFn = fn(&World, &RightClick) -> Option<ClickOrder>;

/// Custom right-click order kinds by name (sorted).
#[derive(Resource, Clone, Default)]
pub struct RightClickOrders {
    pub kinds: Vec<(&'static str, RightClickOrderFn)>,
    /// Kinds whose resolver chooses the units too.
    pub subset_kinds: Vec<(&'static str, RightClickSubsetFn)>,
}

impl RightClickOrders {
    /// Adds (or replaces) kind `name`.
    pub fn add(&mut self, name: &'static str, f: RightClickOrderFn) {
        self.subset_kinds.retain(|(n, _)| *n != name);
        add_sorted(&mut self.kinds, name, f);
    }

    /// Adds (or replaces) kind `name` with a resolver that picks the units.
    pub fn add_subset(&mut self, name: &'static str, f: RightClickSubsetFn) {
        self.kinds.retain(|(n, _)| *n != name);
        add_sorted(&mut self.subset_kinds, name, f);
    }

    /// The plain resolver registered under `name`.
    pub fn get(&self, name: &str) -> Option<RightClickOrderFn> {
        self.kinds
            .binary_search_by_key(&name, |(n, _)| *n)
            .ok()
            .map(|i| self.kinds[i].1)
    }

    /// The order kind `name` gives for `click` (either resolver form).
    pub fn resolve(&self, world: &World, name: &str, click: &RightClick) -> Option<ClickOrder> {
        if let Some(f) = self.get(name) {
            return f(world, click).map(ClickOrder::all);
        }
        let i = self
            .subset_kinds
            .binary_search_by_key(&name, |(n, _)| *n)
            .ok()?;
        (self.subset_kinds[i].1)(world, click)
    }
}

/// Registers right-click resolvers on `App`, in any plugin order (before or
/// after [`CommandUiPlugin`], which keeps what is registered).
pub trait CommandUiAppExt {
    /// [`RightClickOrders::add`]: the custom kind `name`
    /// (`RightClickKind::Custom(name)`).
    fn add_right_click_order(&mut self, name: &'static str, f: RightClickOrderFn) -> &mut Self;
    /// [`RightClickOrders::add_subset`].
    fn add_right_click_order_subset(
        &mut self,
        name: &'static str,
        f: RightClickSubsetFn,
    ) -> &mut Self;
    /// [`ContextOrders::add`]: a context resolver for `Move` clicks on a
    /// target.
    fn add_context_order(&mut self, name: &'static str, f: ContextOrderFn) -> &mut Self;
    /// [`ContextOrders::add_subset`].
    fn add_context_order_subset(
        &mut self,
        name: &'static str,
        f: ContextOrderSubsetFn,
    ) -> &mut Self;
}

impl CommandUiAppExt for App {
    fn add_right_click_order(&mut self, name: &'static str, f: RightClickOrderFn) -> &mut Self {
        self.world_mut()
            .get_resource_or_init::<RightClickOrders>()
            .add(name, f);
        self
    }

    fn add_right_click_order_subset(
        &mut self,
        name: &'static str,
        f: RightClickSubsetFn,
    ) -> &mut Self {
        self.world_mut()
            .get_resource_or_init::<RightClickOrders>()
            .add_subset(name, f);
        self
    }

    fn add_context_order(&mut self, name: &'static str, f: ContextOrderFn) -> &mut Self {
        self.world_mut()
            .get_resource_or_init::<ContextOrders>()
            .add(name, f);
        self
    }

    fn add_context_order_subset(
        &mut self,
        name: &'static str,
        f: ContextOrderSubsetFn,
    ) -> &mut Self {
        self.world_mut()
            .get_resource_or_init::<ContextOrders>()
            .add_subset(name, f);
        self
    }
}

/// The order one right click of `kind` gives. Stateless: `Patrol` is the
/// one-point patrol to the ground (the plugin's [`PatrolDraft`] collects
/// several). `Move` is [`context_order`]; `AttackMove` goes to the clicked
/// unit's position, else the ground; `Custom` asks [`RightClickOrders`]
/// (`None` for an unknown name).
pub fn right_click_order(
    world: &World,
    kind: RightClickKind,
    click: &RightClick,
) -> Option<UnitOrder> {
    right_click_order_for(world, kind, click).map(|c| c.order)
}

/// [`right_click_order`] with the units that take it ([`ClickOrder`]):
/// subset resolvers (`Custom` kinds, context resolvers for `Move`) may
/// narrow the selection; every other order is for the whole selection.
pub fn right_click_order_for(
    world: &World,
    kind: RightClickKind,
    click: &RightClick,
) -> Option<ClickOrder> {
    match kind {
        RightClickKind::Move => context_order_for(world, click),
        RightClickKind::AttackMove => {
            let at = click
                .target
                .and_then(|t| world.get::<SimPos>(t).copied())
                .or(click.ground)?;
            Some(UnitOrder::AttackMove { pos: (at.x, at.z) }.into())
        }
        RightClickKind::Patrol => click.ground.map(|p| {
            UnitOrder::Patrol {
                points: vec![(p.x, p.z)],
            }
            .into()
        }),
        RightClickKind::Custom(name) => world
            .get_resource::<RightClickOrders>()
            .and_then(|o| o.resolve(world, name, click)),
    }
}

/// A patrol click at `ground` (mm): the point joins `draft`; with `add`
/// (Shift) that is all, otherwise the draft becomes the `Patrol` order (and
/// is emptied).
pub fn patrol_click(draft: &mut PatrolDraft, ground: SimPos, add: bool) -> Option<UnitOrder> {
    draft.points.push((ground.x, ground.z));
    (!add).then(|| UnitOrder::Patrol {
        points: std::mem::take(&mut draft.points),
    })
}

/// The selection rectangle in viewport px: the left drag in progress, or one
/// a host set for a scripted selection (screenshots, tutorials). Drawn on
/// the ground while `start` is set and the corners are apart; a left-button
/// release with `start` set box-selects it.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct DragBox {
    pub start: Option<Vec2>,
    pub current: Vec2,
}

/// The command UI's input system (inside `ViewSystems::Input`). A host
/// system that sets [`RightClickMode`] or [`DragBox`] for this frame's
/// clicks runs `.before(NpcCommandUiSet)`.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NpcCommandUiSet;

/// A left click on `picked` (`None`: empty ground). With `add` it toggles
/// the unit; otherwise the selection becomes just it (or nothing).
pub fn click_select(selection: &mut Vec<SimId>, picked: Option<SimId>, add: bool) {
    match (picked, add) {
        (Some(id), true) => match selection.binary_search(&id) {
            Ok(i) => {
                selection.remove(i);
            }
            Err(i) => selection.insert(i, id),
        },
        (None, true) => {}
        (picked, false) => {
            selection.clear();
            selection.extend(picked);
        }
    }
}

/// A box over `inside`: replaces the selection, or with `add` joins it.
pub fn box_select(selection: &mut Vec<SimId>, inside: impl IntoIterator<Item = SimId>, add: bool) {
    if !add {
        selection.clear();
    }
    selection.extend(inside);
    selection.sort_unstable();
    selection.dedup();
}

/// Stores `selection` as group `index` (0 = group 1).
pub fn store_group(groups: &mut ControlGroups, index: usize, selection: &[SimId]) {
    if let Some(g) = groups.groups.get_mut(index) {
        *g = selection.to_vec();
    }
}

/// Group `index`'s units that are still in the sim (the group is pruned).
pub fn recall_group(groups: &mut ControlGroups, index: usize, sims: &SimIndex) -> Vec<SimId> {
    let Some(g) = groups.groups.get_mut(index) else {
        return Vec::new();
    };
    g.retain(|id| sims.get(*id).is_some());
    g.clone()
}

/// The order for a right click: on `target` (resolvers, then `Follow` for
/// a commandable unit, else `Move` to it), else on the `ground`.
pub fn context_order(
    world: &World,
    player: PlayerId,
    target: Option<Entity>,
    ground: Option<SimPos>,
) -> Option<UnitOrder> {
    let click = RightClick {
        player,
        selection: &[],
        target,
        ground,
    };
    context_order_for(world, &click).map(|c| c.order)
}

/// [`context_order`] for a whole click, with the units that take the order
/// (a subset resolver may narrow the selection).
pub fn context_order_for(world: &World, click: &RightClick) -> Option<ClickOrder> {
    if let Some(t) = click.target {
        if let Some(order) = world
            .get_resource::<ContextOrders>()
            .and_then(|c| c.resolve(world, click, t))
        {
            return Some(order);
        }
        if world.get::<Commandable>(t).is_some()
            && let Some(&id) = world.get::<SimId>(t)
        {
            return Some(UnitOrder::Follow { target: id }.into());
        }
        if let Some(p) = world.get::<SimPos>(t) {
            return Some(UnitOrder::Move { pos: (p.x, p.z) }.into());
        }
    }
    click
        .ground
        .map(|p| UnitOrder::Move { pos: (p.x, p.z) }.into())
}

/// The recorded command for `order` to `selection` (`None` when empty).
/// `queue`: shift-queue.
pub fn order_command(
    player: PlayerId,
    selection: &[SimId],
    order: UnitOrder,
    queue: bool,
) -> Option<SimCommand> {
    (!selection.is_empty())
        .then(|| IssueOrder::new(player, selection.iter().copied(), order).queued(queue))
        .map(|cmd| cmd.to_command())
}

/// [`order_command`] with an explicit `formation` for group moves (only a
/// `Move` / `AttackMove` to two or more units carries it).
pub fn order_command_with(
    player: PlayerId,
    selection: &[SimId],
    order: UnitOrder,
    queue: bool,
    formation: Option<Formation>,
) -> Option<SimCommand> {
    if selection.is_empty() {
        return None;
    }
    Some(issue_command(player, selection, order, queue, formation).to_command())
}

fn issue_command(
    player: PlayerId,
    selection: &[SimId],
    order: UnitOrder,
    queue: bool,
    formation: Option<Formation>,
) -> IssueOrder {
    let group_move = selection.len() > 1
        && matches!(order, UnitOrder::Move { .. } | UnitOrder::AttackMove { .. });
    let mut cmd = IssueOrder::new(player, selection.iter().copied(), order).queued(queue);
    if let Some(f) = formation.filter(|_| group_move) {
        cmd = cmd.formation(f);
    }
    cmd
}

/// Where `ray` hits the ground plane (`y = 0`).
pub fn ground_point(ray: Ray3d) -> Option<Vec3> {
    let dir = *ray.direction;
    if dir.y.abs() < 1e-6 {
        return None;
    }
    let t = -ray.origin.y / dir.y;
    (t > 0.0).then(|| ray.origin + dir * t)
}

/// A ground point (m) as a sim position (mm, rounded).
pub fn world_to_sim(v: Vec3) -> SimPos {
    SimPos::new((v.x * 1000.0).round() as i32, (v.z * 1000.0).round() as i32)
}

/// Remembers the selection `Selected` was last synced from.
#[derive(Resource, Clone, Debug, Default)]
struct Synced(Vec<SimId>);

/// The command UI (see the module docs).
pub struct CommandUiPlugin;

impl Plugin for CommandUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CommandUi>()
            .init_resource::<CommandBindings>()
            .init_resource::<CommandSelection>()
            .init_resource::<ControlGroups>()
            .init_resource::<CommandFormation>()
            .init_resource::<ContextOrders>()
            .init_resource::<DragBox>()
            .init_resource::<RightClickMode>()
            .init_resource::<PatrolDraft>()
            .init_resource::<RightClickOrders>()
            .init_resource::<SkipInvalidOrders>()
            .init_resource::<Synced>()
            .init_resource::<Selected>()
            .configure_sets(Update, NpcCommandUiSet.in_set(ViewSystems::Input))
            .add_systems(Update, command_input.in_set(NpcCommandUiSet))
            .add_systems(
                Update,
                (prune_selection, sync_selected)
                    .chain()
                    .in_set(ViewSystems::Sync),
            )
            .add_systems(Update, draw_orders.in_set(ViewSystems::Present));
    }
}

fn pressed(keys: Option<&ButtonInput<KeyCode>>, any: &[KeyCode]) -> bool {
    keys.is_some_and(|k| k.any_pressed(any.iter().copied()))
}

/// `(id, sim entity, feet in world space, owner)` of every selectable unit.
fn candidates(world: &mut World) -> Vec<(SimId, Entity, Vec3, Option<PlayerId>)> {
    let mut visuals = world.query_filtered::<(&VisualOf, &GlobalTransform), With<Selectable>>();
    let rows: Vec<(Entity, Vec3)> = visuals
        .iter(world)
        .map(|(of, t)| (of.0, t.translation()))
        .collect();
    rows_to_candidates(world, rows)
}

/// [`candidates`] plus every [`PickTarget`] visual (right-click targets).
fn pick_candidates(world: &mut World) -> Vec<(SimId, Entity, Vec3, Option<PlayerId>)> {
    let mut visuals = world
        .query_filtered::<(&VisualOf, &GlobalTransform), Or<(With<Selectable>, With<PickTarget>)>>(
        );
    let rows: Vec<(Entity, Vec3)> = visuals
        .iter(world)
        .map(|(of, t)| (of.0, t.translation()))
        .collect();
    rows_to_candidates(world, rows)
}

/// The rows' sim entities as candidates, but those the view hides
/// ([`ViewHidden`]).
fn rows_to_candidates(
    world: &World,
    rows: Vec<(Entity, Vec3)>,
) -> Vec<(SimId, Entity, Vec3, Option<PlayerId>)> {
    let view = world.get_resource::<ViewHidden>();
    rows.into_iter()
        .filter(|(sim, _)| !hidden(view, *sim))
        .filter_map(|(sim, at)| {
            let id = *world.get::<SimId>(sim)?;
            let owner = world.get::<Owner>(sim).map(|o| o.0);
            Some((id, sim, at, owner))
        })
        .collect()
}

/// The unit under `ray` (e.g. `Camera::viewport_to_world` of the cursor):
/// [`pick`](crate::pick) over every [`Selectable`] and [`PickTarget`]
/// visual (its root's position as the feet), as `(id, sim entity)`. Any
/// owner.
pub fn pick_unit(world: &mut World, ray: Ray3d) -> Option<(SimId, Entity)> {
    let all = pick_candidates(world);
    let id = pick(ray.origin, *ray.direction, all.iter().map(|c| (c.0, c.2)))?;
    all.iter().find(|c| c.0 == id).map(|c| (c.0, c.1))
}

/// The primary window's cursor and the 3D camera, if both exist.
fn cursor_and_camera(world: &mut World) -> Option<(Vec2, Camera, GlobalTransform)> {
    let mut windows = world.query_filtered::<&Window, With<PrimaryWindow>>();
    let cursor = windows
        .single(world)
        .ok()
        .and_then(Window::cursor_position)?;
    let (camera, cam) = the_camera(world)?;
    Some((cursor, camera, cam))
}

fn the_camera(world: &mut World) -> Option<(Camera, GlobalTransform)> {
    let mut cameras = world.query_filtered::<(&Camera, &GlobalTransform), With<Camera3d>>();
    cameras.single(world).ok().map(|(c, t)| (c.clone(), *t))
}

/// Where each of `units` is drawn on screen (viewport px, through the 3D
/// camera): their visual roots' positions. Units without a visual, or
/// behind the camera, are left out; order follows `units`.
pub fn unit_screen_positions(world: &mut World, units: &[SimId]) -> Vec<(SimId, Vec2)> {
    let Some((camera, cam)) = the_camera(world) else {
        return Vec::new();
    };
    let (Some(sims), Some(index)) = (
        world.get_resource::<SimIndex>(),
        world.get_resource::<VisualIndex>(),
    ) else {
        return Vec::new();
    };
    units
        .iter()
        .filter_map(|&id| {
            let visual = *index.map.get(&sims.get(id)?)?;
            let at = world.get::<GlobalTransform>(visual)?.translation();
            Some((id, camera.world_to_viewport(&cam, at).ok()?))
        })
        .collect()
}

/// Scripted selection: `units` that the UI's player owns (as a click
/// would), replacing the selection or with `add` joining it. Returns the
/// new selection. `Selected` follows its first unit as usual.
pub fn select_units(
    world: &mut World,
    units: impl IntoIterator<Item = SimId>,
    add: bool,
) -> Vec<SimId> {
    let player = world
        .get_resource::<CommandUi>()
        .copied()
        .unwrap_or_default()
        .player;
    let own: Vec<SimId> = {
        let sims = world.get_resource::<SimIndex>();
        units
            .into_iter()
            .filter(|id| {
                sims.and_then(|s| s.get(*id))
                    .and_then(|e| world.get::<Owner>(e))
                    .is_some_and(|o| o.0 == player)
            })
            .collect()
    };
    let mut sel = world
        .get_resource::<CommandSelection>()
        .map(|s| s.units.clone())
        .unwrap_or_default();
    box_select(&mut sel, own, add);
    let mut current = world.get_resource_or_init::<CommandSelection>();
    if current.units != sel {
        current.units.clone_from(&sel);
    }
    sel
}

/// The UI's own units whose visual lies inside the viewport rectangle with
/// corners `a` and `b` (px), as a left drag would select them
/// ([`select_units`] semantics for `add`). Returns the new selection.
pub fn select_in_rect(world: &mut World, a: Vec2, b: Vec2, add: bool) -> Vec<SimId> {
    let (lo, hi) = (a.min(b), a.max(b));
    let player = world
        .get_resource::<CommandUi>()
        .copied()
        .unwrap_or_default()
        .player;
    let inside: Vec<SimId> = match the_camera(world) {
        Some((camera, cam)) => candidates(world)
            .into_iter()
            .filter(|c| c.3 == Some(player))
            .filter_map(|c| {
                let p = camera.world_to_viewport(&cam, c.2).ok()?;
                (p.cmpge(lo).all() && p.cmple(hi).all()).then_some(c.0)
            })
            .collect(),
        None => Vec::new(),
    };
    select_units(world, inside, add)
}

/// Submits `order` for the current [`CommandSelection`] as the UI's player
/// (`queue`: shift-queue), with [`CommandFormation`] for group moves: the
/// path every UI order takes. `Ok(false)`: nothing selected.
pub fn issue_to_selection(
    world: &mut World,
    order: UnitOrder,
    queue: bool,
) -> Result<bool, CommandError> {
    let selection = world
        .get_resource::<CommandSelection>()
        .map(|s| s.units.clone())
        .unwrap_or_default();
    issue_to_units(world, &selection, order, queue, false).map(|r| r.is_some())
}

/// Submits `order` for `units` as the UI's player (`queue`: shift-queue),
/// with [`CommandFormation`] for group moves; with `skip_invalid` the
/// units that cannot take it are skipped (`IssueOrder::skip_invalid`) and
/// listed in the receipt. `Ok(None)`: no units.
pub fn issue_to_units(
    world: &mut World,
    units: &[SimId],
    order: UnitOrder,
    queue: bool,
    skip_invalid: bool,
) -> Result<Option<IssueReceipt>, CommandError> {
    if units.is_empty() {
        return Ok(None);
    }
    let player = world
        .get_resource::<CommandUi>()
        .copied()
        .unwrap_or_default()
        .player;
    let formation = world.get_resource::<CommandFormation>().and_then(|f| f.0);
    let cmd = issue_command(player, units, order, queue, formation).skip_invalid(skip_invalid);
    submit_order_report(world, &cmd).map(Some)
}

fn issue_logged(world: &mut World, order: UnitOrder, queue: bool) -> bool {
    match issue_to_selection(world, order, queue) {
        Ok(sent) => sent,
        Err(err) => {
            warn!("order rejected: {err}");
            false
        }
    }
}

/// A right-click order: to the resolver's units of the selection, skipping
/// the ones that cannot take it (logged).
fn issue_click_logged(world: &mut World, sel: &[SimId], order: ClickOrder, queue: bool) -> bool {
    let units = order.units_of(sel);
    let skip = world
        .get_resource::<SkipInvalidOrders>()
        .copied()
        .unwrap_or_default()
        .0;
    match issue_to_units(world, &units, order.order, queue, skip) {
        Ok(Some(receipt)) => {
            for s in &receipt.skipped {
                info!("order skipped for unit {}: {}", s.unit.0, s.reason);
            }
            true
        }
        Ok(None) => false,
        Err(err) => {
            warn!("order rejected: {err}");
            false
        }
    }
}

/// Mouse and keys → selection, control groups and recorded orders.
fn command_input(world: &mut World) {
    let ui = world
        .get_resource::<CommandUi>()
        .copied()
        .unwrap_or_default();
    if !ui.enabled {
        return;
    }
    let bindings = world.resource::<CommandBindings>().clone();
    let (Some(buttons), keys) = (
        world.get_resource::<ButtonInput<MouseButton>>().cloned(),
        world.get_resource::<ButtonInput<KeyCode>>().cloned(),
    ) else {
        return;
    };
    let add = pressed(keys.as_ref(), &bindings.add);

    // Control groups.
    if let Some(keys) = keys.as_ref() {
        let store = pressed(Some(keys), &bindings.store_group);
        for (i, key) in bindings.groups.iter().enumerate() {
            if !keys.just_pressed(*key) {
                continue;
            }
            if store {
                let sel = world.resource::<CommandSelection>().units.clone();
                store_group(&mut world.resource_mut::<ControlGroups>(), i, &sel);
            } else {
                let units = world.resource_scope(|world, mut groups: Mut<ControlGroups>| {
                    world
                        .get_resource::<SimIndex>()
                        .map(|sims| recall_group(&mut groups, i, sims))
                        .unwrap_or_default()
                });
                world.resource_mut::<CommandSelection>().units = units;
            }
        }
    }

    // A patrol draft is issued when the mode leaves `Patrol`.
    let mode = world
        .get_resource::<RightClickMode>()
        .copied()
        .unwrap_or_default();
    if mode.kind != RightClickKind::Patrol && !world.resource::<PatrolDraft>().points.is_empty() {
        let points = std::mem::take(&mut world.resource_mut::<PatrolDraft>().points);
        issue_logged(world, UnitOrder::Patrol { points }, false);
    }

    let Some((cursor, camera, cam)) = cursor_and_camera(world) else {
        return;
    };

    if buttons.just_pressed(bindings.select) {
        *world.resource_mut::<DragBox>() = DragBox {
            start: Some(cursor),
            current: cursor,
        };
    }
    if buttons.pressed(bindings.select) {
        world.resource_mut::<DragBox>().current = cursor;
    }
    if buttons.just_released(bindings.select)
        && let Some(start) = world.resource_mut::<DragBox>().start.take()
    {
        if start.distance(cursor) < bindings.drag_px {
            let own: Vec<(SimId, Vec3)> = candidates(world)
                .into_iter()
                .filter(|c| c.3 == Some(ui.player))
                .map(|c| (c.0, c.2))
                .collect();
            let mut sel = world.resource::<CommandSelection>().units.clone();
            let picked = camera
                .viewport_to_world(&cam, cursor)
                .ok()
                .and_then(|ray| pick(ray.origin, *ray.direction, own.iter().copied()));
            click_select(&mut sel, picked, add);
            world.resource_mut::<CommandSelection>().units = sel;
        } else {
            select_in_rect(world, start, cursor, add);
        }
    }

    if buttons.just_pressed(bindings.order) {
        let sel = world.resource::<CommandSelection>().units.clone();
        if sel.is_empty() {
            return;
        }
        let Ok(ray) = camera.viewport_to_world(&cam, cursor) else {
            return;
        };
        let target = pick_unit(world, ray).map(|(_, e)| e);
        let ground = ground_point(ray).map(world_to_sim);
        let order = if mode.kind == RightClickKind::Patrol {
            let Some(ground) = ground else { return };
            let mut draft = world.resource_mut::<PatrolDraft>();
            patrol_click(&mut draft, ground, add).map(ClickOrder::all)
        } else {
            let click = RightClick {
                player: ui.player,
                selection: &sel,
                target,
                ground,
            };
            right_click_order_for(world, mode.kind, &click)
        };
        // A Patrol from the draft replaces the orders (Shift collected it).
        let queue = add && mode.kind != RightClickKind::Patrol;
        if let Some(order) = order
            && issue_click_logged(world, &sel, order, queue)
            && mode.once
        {
            *world.resource_mut::<RightClickMode>() = RightClickMode::default();
        }
    }
}

/// Units that left the sim, or that the view hides ([`ViewHidden`]), leave
/// the selection (so no ring or order gizmo follows them).
fn prune_selection(
    sims: Option<Res<SimIndex>>,
    view: Option<Res<ViewHidden>>,
    mut sel: ResMut<CommandSelection>,
) {
    let Some(sims) = sims else { return };
    let view = view.as_deref();
    if sel.units.iter().any(|id| stale(Some(&sims), view, *id)) {
        sel.units.retain(|id| !stale(Some(&sims), view, *id));
    }
}

/// `Selected` follows the first selected unit whenever the selection changes.
fn sync_selected(
    sel: Res<CommandSelection>,
    mut synced: ResMut<Synced>,
    mut selected: ResMut<Selected>,
) {
    if synced.0 != sel.units {
        synced.0.clone_from(&sel.units);
        selected.0 = sel.units.first().copied();
    }
}

fn order_color(order: &UnitOrder) -> Srgba {
    match order {
        UnitOrder::Move { .. } => css::LIME,
        UnitOrder::AttackMove { .. } => css::RED,
        UnitOrder::Patrol { .. } => css::AQUA,
        UnitOrder::Follow { .. } => css::YELLOW,
        _ => css::WHITE,
    }
}

/// Points an order goes through (for gizmos).
fn order_points(
    order: &UnitOrder,
    sims: Option<&SimIndex>,
    positions: &Query<&SimPos>,
) -> Vec<SimPos> {
    match order {
        UnitOrder::Patrol { points } => points.iter().map(|&(x, z)| SimPos::new(x, z)).collect(),
        other => match other.move_goal() {
            Some(p) => vec![p],
            None => other
                .target()
                .and_then(|t| sims?.get(t))
                .and_then(|e| positions.get(e).ok().copied())
                .into_iter()
                .collect(),
        },
    }
}

fn lift(p: SimPos) -> Vec3 {
    sim_to_world(p) + Vec3::Y * 0.07
}

#[allow(clippy::too_many_arguments)]
fn draw_orders(
    ui: Res<CommandUi>,
    sel: Res<CommandSelection>,
    drag: Res<DragBox>,
    draft: Res<PatrolDraft>,
    sims: Option<Res<SimIndex>>,
    queues: Query<(&SimPos, &OrderQueue)>,
    positions: Query<&SimPos>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut gizmos: Gizmos,
) {
    if !ui.gizmos {
        return;
    }
    let flat = Quat::from_rotation_arc(Vec3::Z, Vec3::Y);
    for id in &sel.units {
        let Some(e) = sims.as_deref().and_then(|s| s.get(*id)) else {
            continue;
        };
        let Ok((pos, queue)) = queues.get(e) else {
            continue;
        };
        gizmos.circle(Isometry3d::new(lift(*pos), flat), 0.6, css::LIME);
        let mut from = lift(*pos);
        for order in queue.orders() {
            let color = order_color(order);
            for p in order_points(order, sims.as_deref(), &positions) {
                let to = lift(p);
                gizmos.line(from, to, color);
                gizmos.circle(Isometry3d::new(to, flat), 0.2, color);
                from = to;
            }
        }
    }
    // The patrol points collected so far.
    let mut prev: Option<Vec3> = None;
    for &(x, z) in &draft.points {
        let at = lift(SimPos::new(x, z));
        gizmos.circle(Isometry3d::new(at, flat), 0.3, css::AQUA);
        if let Some(p) = prev {
            gizmos.line(p, at, css::AQUA);
        }
        prev = Some(at);
    }
    // The drag box, projected onto the ground.
    let (Some(start), Ok((camera, cam))) = (drag.start, cameras.single()) else {
        return;
    };
    if windows.single().is_err() || start.distance(drag.current) < 2.0 {
        return;
    }
    let corners = [
        start,
        Vec2::new(drag.current.x, start.y),
        drag.current,
        Vec2::new(start.x, drag.current.y),
    ];
    let ground: Vec<Vec3> = corners
        .iter()
        .filter_map(|&c| camera.viewport_to_world(cam, c).ok().and_then(ground_point))
        .map(|p| p + Vec3::Y * 0.05)
        .collect();
    if ground.len() == 4 {
        for i in 0..4 {
            gizmos.line(ground[i], ground[(i + 1) % 4], css::LIME);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn click_replaces_and_shift_toggles() {
        let mut sel = vec![SimId(2)];
        click_select(&mut sel, Some(SimId(5)), false);
        assert_eq!(sel, [SimId(5)]);
        click_select(&mut sel, Some(SimId(3)), true);
        assert_eq!(sel, [SimId(3), SimId(5)]);
        click_select(&mut sel, Some(SimId(5)), true);
        assert_eq!(sel, [SimId(3)]);
        click_select(&mut sel, None, true);
        assert_eq!(sel, [SimId(3)]);
        click_select(&mut sel, None, false);
        assert!(sel.is_empty());
    }

    #[test]
    fn box_replaces_or_adds_sorted() {
        let mut sel = vec![SimId(9)];
        box_select(&mut sel, [SimId(4), SimId(1)], false);
        assert_eq!(sel, [SimId(1), SimId(4)]);
        box_select(&mut sel, [SimId(4), SimId(7)], true);
        assert_eq!(sel, [SimId(1), SimId(4), SimId(7)]);
    }

    #[test]
    fn control_groups_store_and_recall_live_units() {
        let mut world = World::new();
        let a = world.spawn_empty().id();
        let mut sims = SimIndex::default();
        sims.map.insert(SimId(1), a);
        let mut groups = ControlGroups::default();
        store_group(&mut groups, 2, &[SimId(1), SimId(2)]);
        assert_eq!(recall_group(&mut groups, 2, &sims), [SimId(1)]);
        assert_eq!(groups.groups[2], [SimId(1)], "pruned");
        assert!(recall_group(&mut groups, 0, &sims).is_empty());
    }

    #[test]
    fn context_orders_resolve_ground_units_and_hooks() {
        fn door_opens(world: &World, _: PlayerId, e: Entity) -> Option<UnitOrder> {
            let id = *world.get::<SimId>(e)?;
            (id == SimId(30)).then(|| UnitOrder::Interact {
                verb: "open".into(),
                target: id,
            })
        }
        let mut world = World::new();
        world.init_resource::<ContextOrders>();
        let unit = world
            .spawn((SimId(10), SimPos::new(1, 2), Commandable::default()))
            .id();
        let rock = world.spawn((SimId(20), SimPos::new(3, 4))).id();
        let door = world.spawn((SimId(30), SimPos::new(5, 6))).id();
        let p = PlayerId(1);
        let ground = Some(SimPos::new(7, 8));
        assert_eq!(
            context_order(&world, p, None, ground),
            Some(UnitOrder::Move { pos: (7, 8) })
        );
        assert_eq!(
            context_order(&world, p, Some(unit), ground),
            Some(UnitOrder::Follow { target: SimId(10) })
        );
        assert_eq!(
            context_order(&world, p, Some(rock), ground),
            Some(UnitOrder::Move { pos: (3, 4) })
        );
        world
            .resource_mut::<ContextOrders>()
            .add("doors", door_opens);
        assert!(matches!(
            context_order(&world, p, Some(door), ground),
            Some(UnitOrder::Interact { .. })
        ));
    }

    #[test]
    fn orders_become_recorded_commands() {
        assert!(order_command(PlayerId(1), &[], UnitOrder::Stop, false).is_none());
        let cmd = order_command(PlayerId(1), &[SimId(4)], UnitOrder::Hold, true).unwrap();
        assert_eq!(cmd.kind(), "IssueOrder");
        let json = serde_json::to_value(&cmd).unwrap();
        assert_eq!(json["player"], 1);
        assert_eq!(json["queue"], true);
        assert_eq!(json["order"]["type"], "Hold");
    }

    #[test]
    fn group_moves_carry_the_ui_formation() {
        let line = Formation::new(npc_command::FormationKind::Line, 2000);
        let one = order_command_with(
            PlayerId(1),
            &[SimId(4)],
            UnitOrder::Move { pos: (1, 2) },
            false,
            Some(line),
        )
        .unwrap();
        let json = serde_json::to_value(&one).unwrap();
        assert!(json.get("formation").is_none(), "one unit: no formation");
        let group = order_command_with(
            PlayerId(1),
            &[SimId(4), SimId(5)],
            UnitOrder::Move { pos: (1, 2) },
            true,
            Some(line),
        )
        .unwrap();
        let json = serde_json::to_value(&group).unwrap();
        assert_eq!(json["formation"]["kind"], "Line");
        assert_eq!(json["formation"]["spacing_mm"], 2000);
        let hold = order_command_with(
            PlayerId(1),
            &[SimId(4), SimId(5)],
            UnitOrder::Hold,
            false,
            Some(line),
        )
        .unwrap();
        assert!(
            serde_json::to_value(&hold)
                .unwrap()
                .get("formation")
                .is_none()
        );
    }

    #[test]
    fn ground_hits_and_sim_positions() {
        let ray = Ray3d::new(Vec3::new(1.0, 10.0, 2.0), Dir3::NEG_Y);
        let p = ground_point(ray).unwrap();
        assert_eq!(world_to_sim(p), SimPos::new(1000, 2000));
        assert!(ground_point(Ray3d::new(Vec3::Y, Dir3::Y)).is_none());
    }

    #[test]
    fn right_click_kinds_choose_the_order() {
        fn regroup(_: &World, click: &RightClick) -> Option<UnitOrder> {
            let g = click.ground?;
            (click.selection.len() > 1).then(|| UnitOrder::Move { pos: (g.x, -g.z) })
        }
        let mut world = World::new();
        let unit = world
            .spawn((SimId(10), SimPos::new(1, 2), Commandable::default()))
            .id();
        let sel = [SimId(1), SimId(2)];
        let click = |target| RightClick {
            player: PlayerId(1),
            selection: &sel,
            target,
            ground: Some(SimPos::new(7, 8)),
        };
        // Move: the context order (ground / a commandable unit).
        assert_eq!(
            right_click_order(&world, RightClickKind::Move, &click(None)),
            Some(UnitOrder::Move { pos: (7, 8) })
        );
        assert_eq!(
            right_click_order(&world, RightClickKind::Move, &click(Some(unit))),
            Some(UnitOrder::Follow { target: SimId(10) })
        );
        // AttackMove: the unit's position, else the ground.
        assert_eq!(
            right_click_order(&world, RightClickKind::AttackMove, &click(Some(unit))),
            Some(UnitOrder::AttackMove { pos: (1, 2) })
        );
        assert_eq!(
            right_click_order(&world, RightClickKind::AttackMove, &click(None)),
            Some(UnitOrder::AttackMove { pos: (7, 8) })
        );
        // Patrol (stateless): one point at the ground.
        assert_eq!(
            right_click_order(&world, RightClickKind::Patrol, &click(None)),
            Some(UnitOrder::Patrol {
                points: vec![(7, 8)]
            })
        );
        // Custom kinds by name; an unknown name gives nothing.
        let custom = RightClickKind::Custom("regroup");
        assert_eq!(right_click_order(&world, custom, &click(None)), None);
        world.init_resource::<RightClickOrders>();
        world
            .resource_mut::<RightClickOrders>()
            .add("regroup", regroup);
        assert_eq!(
            right_click_order(&world, custom, &click(None)),
            Some(UnitOrder::Move { pos: (7, -8) })
        );
        assert_eq!(
            right_click_order(&world, RightClickKind::Custom("nope"), &click(None)),
            None
        );
    }

    #[test]
    fn subset_resolvers_narrow_the_selection() {
        fn evens(_: &World, click: &RightClick) -> Option<ClickOrder> {
            let units = click.selection.iter().copied().filter(|id| id.0 % 2 == 0);
            Some(ClickOrder::only(units, UnitOrder::Hold))
        }
        fn wall(world: &World, click: &RightClick, e: Entity) -> Option<ClickOrder> {
            let id = *world.get::<SimId>(e)?;
            (id == SimId(30)).then(|| {
                ClickOrder::only(
                    click.selection.first().copied(),
                    UnitOrder::Custom {
                        name: "repair".into(),
                        args: Default::default(),
                    },
                )
            })
        }
        fn plain_b(_: &World, _: PlayerId, _: Entity) -> Option<UnitOrder> {
            Some(UnitOrder::Stop)
        }
        let mut app = App::new();
        // Before the plugin: the AppExt initialises the tables, the plugin keeps them.
        app.add_right_click_order_subset("evens", evens)
            .add_context_order_subset("a_wall", wall)
            .add_context_order("b_plain", plain_b);
        app.add_plugins(CommandUiPlugin);
        let world = app.world_mut();
        let w = world.spawn((SimId(30), SimPos::new(5, 6))).id();
        let other = world.spawn((SimId(31), SimPos::new(5, 6))).id();
        let sel = [SimId(1), SimId(2), SimId(4)];
        let click = |target| RightClick {
            player: PlayerId(0),
            selection: &sel,
            target,
            ground: Some(SimPos::new(7, 8)),
        };
        let custom =
            right_click_order_for(world, RightClickKind::Custom("evens"), &click(None)).unwrap();
        assert_eq!(custom.order, UnitOrder::Hold);
        assert_eq!(custom.units_of(&sel), [SimId(2), SimId(4)]);
        // The old signature still answers (the order only).
        assert_eq!(
            right_click_order(world, RightClickKind::Custom("evens"), &click(None)),
            Some(UnitOrder::Hold)
        );
        // Context: subset and plain resolvers in name order (a_wall, then b_plain).
        let on_wall = right_click_order_for(world, RightClickKind::Move, &click(Some(w))).unwrap();
        assert_eq!(on_wall.units_of(&sel), [SimId(1)]);
        assert!(matches!(on_wall.order, UnitOrder::Custom { .. }));
        let on_other =
            right_click_order_for(world, RightClickKind::Move, &click(Some(other))).unwrap();
        assert_eq!(on_other, ClickOrder::all(UnitOrder::Stop));
        assert_eq!(on_other.units_of(&sel), sel);
        // A subset outside the selection gives nobody.
        assert!(
            ClickOrder::only([SimId(9)], UnitOrder::Stop)
                .units_of(&sel)
                .is_empty()
        );
    }

    #[test]
    fn pick_targets_are_picked_but_never_selected() {
        let mut world = World::new();
        let wall = world.spawn((SimId(7), Owner(PlayerId(0)))).id();
        world.spawn((
            VisualOf(wall),
            PickTarget,
            GlobalTransform::from_translation(Vec3::new(1.0, 0.0, 0.0)),
        ));
        let origin = Vec3::new(1.0, 10.0, 10.0);
        let body = Vec3::new(1.0, crate::PICK_HEIGHT, 0.0);
        let ray = Ray3d::new(origin, Dir3::new(body - origin).unwrap());
        assert_eq!(pick_unit(&mut world, ray), Some((SimId(7), wall)));
        assert!(candidates(&mut world).is_empty(), "not selectable");
    }

    #[test]
    fn patrol_clicks_collect_then_issue() {
        let mut draft = PatrolDraft::default();
        assert_eq!(patrol_click(&mut draft, SimPos::new(1, 2), true), None);
        assert_eq!(patrol_click(&mut draft, SimPos::new(3, 4), true), None);
        assert_eq!(draft.points, [(1, 2), (3, 4)]);
        assert_eq!(
            patrol_click(&mut draft, SimPos::new(5, 6), false),
            Some(UnitOrder::Patrol {
                points: vec![(1, 2), (3, 4), (5, 6)]
            })
        );
        assert!(draft.points.is_empty(), "issued drafts start over");
        assert_eq!(
            patrol_click(&mut draft, SimPos::new(9, 9), false),
            Some(UnitOrder::Patrol {
                points: vec![(9, 9)]
            }),
            "one plain click: a one-point patrol"
        );
        assert_eq!(
            RightClickMode::once(RightClickKind::AttackMove),
            RightClickMode {
                kind: RightClickKind::AttackMove,
                once: true
            }
        );
        assert_eq!(RightClickMode::default().kind, RightClickKind::Move);
    }

    #[test]
    fn scripted_selection_keeps_own_units() {
        let mut world = World::new();
        let mut sims = SimIndex::default();
        for (id, player) in [(1, 0), (2, 1), (3, 0)] {
            let e = world.spawn((SimId(id), Owner(PlayerId(player)))).id();
            sims.map.insert(SimId(id), e);
        }
        world.insert_resource(sims);
        world.insert_resource(CommandUi::default()); // player 0
        world.init_resource::<CommandSelection>();
        let sel = select_units(&mut world, [SimId(3), SimId(2), SimId(9), SimId(1)], false);
        assert_eq!(sel, [SimId(1), SimId(3)], "own units only, sorted");
        assert_eq!(world.resource::<CommandSelection>().units, sel);
        let sel = select_units(&mut world, [SimId(2)], true);
        assert_eq!(
            sel,
            [SimId(1), SimId(3)],
            "another player's unit is not added"
        );
        assert!(select_units(&mut world, [], false).is_empty());
        // No camera: nothing is inside any rectangle.
        assert!(select_in_rect(&mut world, Vec2::ZERO, Vec2::splat(1e4), false).is_empty());
        assert!(unit_screen_positions(&mut world, &[SimId(1)]).is_empty());
    }

    #[test]
    fn pick_unit_finds_the_visual_under_the_ray() {
        let mut world = World::new();
        let near = world.spawn((SimId(1), Owner(PlayerId(1)))).id();
        let far = world.spawn(SimId(2)).id();
        for (sim, x) in [(near, 1.0), (far, 5.0)] {
            world.spawn((
                VisualOf(sim),
                Selectable,
                GlobalTransform::from_translation(Vec3::new(x, 0.0, 0.0)),
            ));
        }
        // An unselectable visual right under the ray is ignored.
        world.spawn((VisualOf(far), GlobalTransform::default()));
        let origin = Vec3::new(1.0, 10.0, 10.0);
        let body = Vec3::new(1.0, crate::PICK_HEIGHT, 0.0);
        let ray = Ray3d::new(origin, Dir3::new(body - origin).unwrap());
        assert_eq!(pick_unit(&mut world, ray), Some((SimId(1), near)));
        let up = Ray3d::new(origin, Dir3::Y);
        assert_eq!(pick_unit(&mut world, up), None);
        // A unit the view hides (fog of war) cannot be picked.
        world.insert_resource(ViewHidden([near].into_iter().collect()));
        assert_eq!(pick_unit(&mut world, ray), None);
    }

    /// Hidden units are not candidates and leave the selection.
    #[test]
    fn hidden_units_leave_the_selection() {
        let mut app = App::new();
        let (a, b) = (
            app.world_mut().spawn(SimId(1)).id(),
            app.world_mut().spawn(SimId(2)).id(),
        );
        let mut sims = SimIndex::default();
        sims.map.insert(SimId(1), a);
        sims.map.insert(SimId(2), b);
        app.insert_resource(sims)
            .insert_resource(CommandSelection {
                units: vec![SimId(1), SimId(2)],
            })
            .insert_resource(ViewHidden([b].into_iter().collect()))
            .add_systems(Update, prune_selection);
        app.update();
        assert_eq!(app.world().resource::<CommandSelection>().units, [SimId(1)]);
        let rows = vec![(a, Vec3::ZERO), (b, Vec3::X)];
        let ids: Vec<SimId> = rows_to_candidates(app.world(), rows)
            .iter()
            .map(|c| c.0)
            .collect();
        assert_eq!(ids, [SimId(1)]);
        assert!(stale(
            app.world().get_resource(),
            app.world().get_resource(),
            SimId(2)
        ));
        assert!(!stale(
            app.world().get_resource(),
            app.world().get_resource(),
            SimId(1)
        ));
        assert!(stale(app.world().get_resource(), None, SimId(9)), "gone");
    }

    #[test]
    fn selection_orders_go_through_submit() {
        use npc_kernel::spatial::SpatialPlugin;
        let mut app = App::new();
        app.add_plugins((
            npc_kernel::NpcKernelPlugin::default(),
            SpatialPlugin,
            npc_command::CommandPlugin,
        ));
        app.finish();
        app.cleanup();
        let world = app.world_mut();
        let (a, _) = npc_kernel::spawn_sim(
            world,
            (SimPos::new(0, 0), Commandable::new(100), Owner(PlayerId(0))),
        );
        world.insert_resource(CommandUi::default());
        world.init_resource::<CommandSelection>();
        assert_eq!(
            issue_to_selection(world, UnitOrder::Hold, false),
            Ok(false),
            "nothing selected"
        );
        select_units(world, [a], false);
        assert_eq!(issue_to_selection(world, UnitOrder::Hold, true), Ok(true));
        let queue = world.resource::<npc_kernel::commands::CommandQueue>();
        let json: Vec<serde_json::Value> = queue
            .pending
            .values()
            .map(|c| serde_json::to_value(c).unwrap())
            .collect();
        assert_eq!(json.len(), 1);
        assert_eq!(json[0]["type"], "IssueOrder");
        assert_eq!(json[0]["queue"], true);
        assert_eq!(json[0]["order"]["type"], "Hold");
    }
}
