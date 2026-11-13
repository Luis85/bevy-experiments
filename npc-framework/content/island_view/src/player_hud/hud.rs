//! The Live HUD's chrome: the goal tracker, the needs, the hotbar and the
//! prompt. Presentation only: every control queues a [`PlayerAction`], and
//! what they show is read from the played actor's own observation.
use super::*;
use crate::game_ui::icon::{self, Icon};
use crate::game_ui::scale::{self, LayoutClass};
use crate::game_ui::tokens::{self as t, Status, Surface};
use crate::game_ui::tooltip::Tooltip;
use crate::game_ui::widgets::{self as w, Bar, ButtonSpec, Kind, NeedBar, Reach, Size, Tone};
use crate::island_shell::focus::FocusRegion;

/// Health and the four needs, in the order the panel lists them:
/// observation key, name, icon.
pub(super) const NEEDS: [(&str, &str, Icon); 5] = [
    ("health", "Health", Icon::Health),
    ("hunger", "Food", Icon::Food),
    ("thirst", "Water", Icon::Water),
    ("energy", "Energy", Icon::Rest),
    ("warmth", "Warmth", Icon::Warmth),
];
/// Width of the needs panel and of the goal tracker.
const NEEDS_WIDTH: f32 = 216.0;
const GOAL_WIDTH: f32 = 300.0;
/// Below these a need is acted on, and interrupts everything else
/// (milli-units of 100 000): the levels the HUD has always warned at.
const LOW: u64 = 45_000;
const CRITICAL: u64 = 20_000;

/// How pressing a need is: fine, low (a triangle) or critical (a diamond).
pub(super) fn need_status(value: u64) -> Status {
    if value < CRITICAL {
        Status::Crit
    } else if value < LOW {
        Status::Warn
    } else {
        Status::Good
    }
}

/// The tooltip's sentence for a need: the percentage lives here, not on the bar.
pub(super) fn need_sentence(name: &str, value: u64) -> String {
    let percent = (value / 1000).min(100);
    match need_status(value) {
        Status::Crit => format!("{name} is at {percent}%: dangerously low. See to it now."),
        Status::Warn => format!("{name} is at {percent}%: low. See to it soon."),
        _ => format!("{name} is at {percent}%."),
    }
}

/// The hotbar's left edge and tile width for a window `width` wide: seven
/// workspaces, a rule and Stop, centred.
pub(super) fn hotbar_rect(width: f32, class: LayoutClass) -> (f32, f32, f32) {
    let tile = if class == LayoutClass::Wide {
        t::DOCK_TILE.0
    } else {
        t::DOCK_TILE_STANDARD
    };
    let (padding, gap) = (5.0, 4.0);
    // Eight tiles, eight gaps around the rule between them, the rule itself.
    let total = 2.0 * padding + 8.0 * tile + 8.0 * gap + 1.0 + 2.0;
    (((width - total) * 0.5).round().max(0.0), tile, total)
}

/// Where the prompt lane stands: `(left, right, bottom)` insets. Centred
/// above the hotbar; beside a workspace so wide that the pill would run
/// under it, above the needs instead, in what the workspace leaves free.
pub(super) fn prompt_lane(width: f32, workspace: f32) -> (f32, f32, f32) {
    // Half of the longest pill ("Walking, then: ..." with Stop).
    const HALF_PILL: f32 = 160.0;
    // Five needs and the purse at 22 each, their gaps, padding and edge.
    const NEEDS_HEIGHT: f32 = 152.0;
    let above_hotbar = t::SCREEN_MARGIN + t::DOCK + 16.0;
    if workspace <= 0.0 || width * 0.5 + HALF_PILL <= width - workspace - t::SCREEN_MARGIN {
        (0.0, 0.0, above_hotbar)
    } else {
        (
            t::SCREEN_MARGIN,
            workspace + t::SCREEN_MARGIN,
            t::SCREEN_MARGIN + NEEDS_HEIGHT + t::SCREEN_MARGIN,
        )
    }
}

/// What the pill above the hotbar says.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Prompt {
    /// Nothing in reach and nothing under way: no pill at all.
    None,
    /// Something in reach: the key, then the verb and the object.
    Reach { text: String, enabled: bool },
    /// The character is on the way or at work: what, and how to stop.
    Working { text: String },
}

/// The pill for this moment. Work under way comes first; then what is in
/// reach; a paused world says so in the top bar, not here.
pub(super) fn prompt(
    context: Option<(&str, bool)>,
    pending: Option<&str>,
    busy: Option<&str>,
) -> Prompt {
    if let Some(label) = pending {
        return Prompt::Working {
            text: format!("Walking, then: {}", lower_first(label)),
        };
    }
    if let Some(activity) = busy {
        return Prompt::Working {
            text: capitalised(activity),
        };
    }
    match context {
        Some((text, enabled)) => Prompt::Reach {
            text: text.to_owned(),
            enabled,
        },
        None => Prompt::None,
    }
}

/// "Step 2 of 6", or "All 7 done".
pub(super) fn chain_words(chain: &super::depth::Chain) -> String {
    if chain.done >= chain.total {
        format!("All {} done", chain.total)
    } else {
        format!("Step {} of {}", chain.step, chain.total)
    }
}

/// The chain's line under the goal: the current goal and what it unlocks.
pub(super) fn chain_reward(chain: &super::depth::Chain) -> String {
    match (&chain.current, chain.reward.is_empty()) {
        (Some(goal), true) => format!("Goal: {goal}"),
        (Some(goal), false) => format!("Goal: {goal}. {}.", chain.reward),
        (None, _) => String::new(),
    }
}

fn capitalised(text: &str) -> String {
    let mut letters = text.chars();
    letters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(letters).collect()
    })
}

fn lower_first(text: &str) -> String {
    let mut letters = text.chars();
    letters.next().map_or_else(String::new, |first| {
        first.to_lowercase().chain(letters).collect()
    })
}

/// What the goal tracker's one button is called: it names what it does.
pub(super) fn goal_button(action: &Action) -> String {
    match action {
        Action::Open(tab) => format!("Open {}", tab.label()),
        Action::Work(request) if request.approach.is_some() => "Go and do it".to_owned(),
        Action::Work(_) => "Do it now".to_owned(),
        _ => "Take the next step".to_owned(),
    }
}

#[derive(Clone, Copy)]
struct GoalParts {
    caption: Entity,
    /// "Step 2 of 6" at the right of the caption, the pips and the reward.
    step: Entity,
    pips: Entity,
    reward: Entity,
    title: Entity,
    detail: Entity,
    progress: Entity,
    bar: Bar,
    button: Entity,
    fold_icon: Entity,
}

#[derive(Clone, Copy)]
struct PromptParts {
    reach: Entity,
    reach_text: Entity,
    working: Entity,
    working_text: Entity,
    /// The share of the work done, where the simulation knows its length.
    working_bar: Bar,
    /// "3 of 10" of a repeat order.
    working_count: Entity,
}

/// What the HUD spawned, and what it last showed.
#[derive(Resource, Default)]
pub(super) struct Hud {
    needs: Vec<(&'static str, Entity, NeedBar)>,
    coins: Option<Entity>,
    goal: Option<GoalParts>,
    prompt: Option<PromptParts>,
    tiles: Vec<(Option<Tab>, Entity)>,
    /// The outcome sentence last handed to the toast lane.
    feedback: Option<String>,
    folded_icon: Option<bool>,
    /// Goals of the chain done when last seen: one more is said as a toast.
    goals_done: Option<u32>,
}

#[derive(Component)]
pub(super) struct NeedsRoot;
/// A need row below the interrupt level.
#[derive(Component)]
pub(super) struct CriticalNeed;

/// How strongly a critical need's outline shows at `seconds` of view
/// time: one short pulse every two seconds, or a steady edge when motion
/// is reduced. Never zero, so the state does not depend on the moment.
pub(super) fn pulse_alpha(seconds: f32, reduce_motion: bool) -> f32 {
    if reduce_motion {
        return 0.9;
    }
    let phase = (seconds % 2.0) / 2.0;
    let wave = (std::f32::consts::PI * phase).sin().powi(6);
    0.35 + 0.6 * wave
}

/// The one motion of the HUD: a critical need's row pulses (spec 5.6).
/// Reduce motion in Settings makes the edge steady. View time only.
pub(super) fn pulse(
    time: Res<Time>,
    motion: Option<Res<crate::ReduceMotion>>,
    mut rows: Query<(Entity, Option<&mut Outline>), With<CriticalNeed>>,
    mut commands: Commands,
) {
    let alpha = pulse_alpha(time.elapsed_secs(), motion.is_some_and(|motion| motion.0));
    let color = t::CRIT_LIGHT.with_alpha(alpha);
    for (entity, outline) in &mut rows {
        match outline {
            Some(mut outline) => outline.color = color,
            None => {
                commands
                    .entity(entity)
                    .insert(Outline::new(px(2), px(0), color));
            }
        }
    }
}
#[derive(Component)]
pub(super) struct PromptRoot;

fn set_node(world: &mut World, entity: Entity, edit: impl FnOnce(&mut Node)) {
    if let Some(mut node) = world.get_mut::<Node>(entity) {
        let mut next = node.clone();
        edit(&mut next);
        if *node != next {
            *node = next;
        }
    }
}

fn tip(world: &mut World, entity: Entity, tooltip: Tooltip) {
    if world.get::<Tooltip>(entity) != Some(&tooltip) {
        world.entity_mut(entity).insert(tooltip);
    }
}

pub(super) fn spawn(world: &mut World) {
    world.init_resource::<Hud>();
    let mut hud = Hud::default();

    // Goal tracker, top left under the bar.
    let goal = root(
        world,
        "Live goal",
        w::chrome(
            Node {
                position_type: PositionType::Absolute,
                left: px(t::SCREEN_MARGIN),
                top: px(t::TOP_BAR + t::SCREEN_MARGIN),
                width: px(GOAL_WIDTH),
                max_width: percent(40),
                padding: UiRect::axes(px(12), px(8)),
                row_gap: px(4),
                flex_direction: FlexDirection::Column,
                display: Display::None,
                ..default()
            },
            t::RADIUS_LG,
        ),
    );
    world
        .entity_mut(goal)
        .insert((JourneyRoot, FocusRegion::LiveHud));
    let head = world
        .spawn((
            ChildOf(goal),
            Node {
                align_items: AlignItems::Center,
                column_gap: px(6),
                min_height: px(22),
                ..default()
            },
        ))
        .id();
    icon::icon(
        world,
        head,
        Icon::Goal,
        14.0,
        Tone::secondary(Surface::Chrome),
    );
    let caption = w::label(
        world,
        head,
        "",
        t::CAPTION,
        Tone::secondary(Surface::Chrome),
    );
    world.entity_mut(caption).insert(Node {
        flex_grow: 1.0,
        flex_shrink: 1.0,
        min_width: px(0),
        overflow: Overflow::clip_x(),
        ..default()
    });
    let step = w::label(world, head, "", t::CAPTION, Tone::caption(Surface::Chrome));
    world.entity_mut(step).insert(Node {
        flex_shrink: 0.0,
        ..default()
    });
    let fold = w::control(
        world,
        head,
        "Fold goal",
        Kind::Quiet,
        Surface::Chrome,
        Reach::Ring,
        Node {
            width: px(22),
            height: px(22),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(t::RADIUS_SM)),
            flex_shrink: 0.0,
            ..default()
        },
        PlayerAction(Action::GoalFold),
    );
    let fold_icon = world
        .spawn((
            ChildOf(fold),
            Node {
                width: px(10),
                height: px(10),
                ..default()
            },
        ))
        .id();
    tip(
        world,
        fold,
        Tooltip::new("Goal", "Fold the goal to its title, or open it again."),
    );
    let title = w::paragraph(world, goal, "", t::SUBHEADING, Tone::text(Surface::Chrome));
    let detail = w::paragraph(world, goal, "", t::DENSE, Tone::secondary(Surface::Chrome));
    let progress = world
        .spawn((
            ChildOf(goal),
            Node {
                flex_direction: FlexDirection::Column,
                display: Display::None,
                ..default()
            },
        ))
        .id();
    let bar = w::progress_bar(world, progress, Surface::Chrome, 0.0, Some(""));
    // The chain: one pip a goal, and what the current one unlocks.
    let pips = w::pips(world, goal);
    set_node(world, pips, |node| {
        node.display = Display::None;
        node.margin = UiRect::vertical(px(2));
    });
    let reward = w::paragraph(
        world,
        goal,
        "",
        t::CAPTION,
        Tone::secondary(Surface::Chrome),
    );
    let button = w::button(
        world,
        goal,
        ButtonSpec::new("Take the next step", Kind::Primary, Surface::Chrome)
            .size(Size::Small)
            .reach(Reach::Ring),
        PlayerAction(Action::Goal),
    );
    set_node(world, button, |node| {
        node.align_self = AlignSelf::FlexStart;
        node.margin = UiRect::top(px(2));
    });
    hud.goal = Some(GoalParts {
        caption,
        step,
        pips,
        reward,
        title,
        detail,
        progress,
        bar,
        button,
        fold_icon,
    });

    // Needs, bottom left: icon, name and a bar each; then the purse.
    let needs = root(
        world,
        "Live needs",
        w::chrome(
            Node {
                position_type: PositionType::Absolute,
                left: px(t::SCREEN_MARGIN),
                bottom: px(t::SCREEN_MARGIN),
                width: px(NEEDS_WIDTH),
                padding: UiRect::axes(px(6), px(6)),
                row_gap: px(1),
                flex_direction: FlexDirection::Column,
                display: Display::None,
                ..default()
            },
            t::RADIUS_LG,
        ),
    );
    world
        .entity_mut(needs)
        .insert((NeedsRoot, FocusRegion::LiveHud));
    for (key, name, picture) in NEEDS {
        let row = w::control(
            world,
            needs,
            name,
            Kind::Quiet,
            Surface::Chrome,
            Reach::Ring,
            Node {
                height: px(22),
                padding: UiRect::horizontal(px(6)),
                align_items: AlignItems::Center,
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(t::RADIUS_SM)),
                ..default()
            },
            PlayerAction(Action::NeedHelp(Some(key))),
        );
        let bar = w::need_bar(
            world,
            row,
            Surface::Chrome,
            picture,
            name,
            1.0,
            Status::Good,
        );
        set_node(world, bar.bar.entity, |node| {
            node.flex_grow = 1.0;
            node.min_width = px(0);
        });
        hud.needs.push((key, row, bar));
    }
    let purse = world
        .spawn((
            ChildOf(needs),
            Node {
                height: px(24),
                padding: UiRect::horizontal(px(7)),
                column_gap: px(6),
                align_items: AlignItems::Center,
                ..default()
            },
        ))
        .id();
    icon::icon(
        world,
        purse,
        Icon::Coins,
        16.0,
        Tone::secondary(Surface::Chrome),
    );
    let name = w::label(world, purse, "Coins", t::DENSE, Tone::text(Surface::Chrome));
    world.entity_mut(name).insert(Node {
        flex_grow: 1.0,
        ..default()
    });
    hud.coins = Some(w::label(
        world,
        purse,
        "0",
        t::BODY,
        Tone::text(Surface::Chrome),
    ));

    // Hotbar, bottom centre: the seven workspaces, a rule, Stop.
    let hotbar = root(
        world,
        "Live hotbar",
        w::chrome(
            Node {
                position_type: PositionType::Absolute,
                left: px(232),
                bottom: px(t::SCREEN_MARGIN),
                height: px(t::DOCK),
                padding: UiRect::horizontal(px(5)),
                column_gap: px(4),
                align_items: AlignItems::Center,
                display: Display::None,
                ..default()
            },
            t::RADIUS_LG,
        ),
    );
    world
        .entity_mut(hotbar)
        .insert((ToolbarRoot, FocusRegion::LiveHotbar));
    for tab in Tab::ALL {
        let tile = w::dock_tile(
            world,
            hotbar,
            tab.label(),
            tab.icon(),
            Some(tab.key()),
            PlayerAction(Action::Toggle(tab)),
        );
        tip(
            world,
            tile.entity,
            Tooltip::new(tab.title(), tab.subtitle())
                .icon(tab.icon())
                .key(tab.key()),
        );
        hud.tiles.push((Some(tab), tile.entity));
    }
    w::divider(world, hotbar, Surface::Chrome, true);
    let stop = w::dock_tile(
        world,
        hotbar,
        "Stop",
        Icon::Stop,
        Some("X"),
        PlayerAction(Action::Stop),
    );
    tip(
        world,
        stop.entity,
        Tooltip::new("Stop", "Stop walking or working, and stand where you are.")
            .icon(Icon::Stop)
            .key("X"),
    );
    hud.tiles.push((None, stop.entity));

    // Prompt, centred above the hotbar: only while something is in reach,
    // or while the character is on the way or at work.
    let lane = world
        .spawn((
            Name::new("Live prompt"),
            PromptRoot,
            Pickable::IGNORE,
            GlobalZIndex(t::layer::CHROME),
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                right: px(0),
                bottom: px(t::SCREEN_MARGIN + t::DOCK + 16.0),
                justify_content: JustifyContent::Center,
                display: Display::None,
                ..default()
            },
        ))
        .id();
    let reach = w::control(
        world,
        lane,
        "Use what is in reach",
        Kind::Secondary,
        Surface::Chrome,
        Reach::Ring,
        Node {
            height: px(36),
            padding: UiRect::horizontal(px(12)),
            column_gap: px(8),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(18)),
            align_items: AlignItems::Center,
            display: Display::None,
            ..default()
        },
        (PlayerRoot, PlayerAction(Action::Goal)),
    );
    // Its words change with what is in reach; the tooltip says what the
    // pill is for whatever they are.
    world.entity_mut(reach).insert(
        Tooltip::new(
            "Use what is in reach",
            "Do what the prompt says with the thing beside you.",
        )
        .key("E"),
    );
    let cap = w::keycap(world, reach, Surface::Chrome, "E");
    world.entity_mut(cap).insert(w::Part::Key);
    let reach_text = w::label(world, reach, "", t::BODY, Tone::text(Surface::Chrome));
    world.entity_mut(reach_text).insert(w::Part::Label);
    let working = world
        .spawn((
            ChildOf(lane),
            PlayerRoot,
            w::chrome(
                Node {
                    height: px(36),
                    padding: UiRect::new(px(14), px(4), px(0), px(0)),
                    column_gap: px(10),
                    align_items: AlignItems::Center,
                    display: Display::None,
                    ..default()
                },
                18.0,
            ),
        ))
        .id();
    let working_text = w::label(world, working, "", t::BODY, Tone::text(Surface::Chrome));
    let working_bar = w::progress_bar(world, working, Surface::Chrome, 0.0, None);
    set_node(world, working_bar.entity, |node| {
        node.width = px(96);
        node.display = Display::None;
    });
    let working_count = w::label(
        world,
        working,
        "",
        t::DENSE,
        Tone::secondary(Surface::Chrome),
    );
    let halt = w::button(
        world,
        working,
        ButtonSpec::new("Stop", Kind::Quiet, Surface::Chrome)
            .size(Size::Small)
            .key("X")
            .reach(Reach::Ring),
        PlayerAction(Action::Stop),
    );
    set_node(world, halt, |node| {
        node.border_radius = BorderRadius::all(px(14));
    });
    hud.prompt = Some(PromptParts {
        reach,
        reach_text,
        working,
        working_text,
        working_bar,
        working_count,
    });
    *world.resource_mut::<Hud>() = hud;
}

/// Place the chrome for the window as it is now: under the bar however tall
/// it is, the hotbar centred, the needs clear of it.
pub(super) fn place(world: &mut World) {
    let layout = scale::layout(world);
    let top = crate::island_shell::content_top(world);
    let (left, tile, total) = hotbar_rect(layout.width, layout.class);
    let tiles = world.resource::<Hud>().tiles.clone();
    let journey: Vec<Entity> = world
        .query_filtered::<Entity, With<JourneyRoot>>()
        .iter(world)
        .collect();
    for entity in journey {
        set_node(world, entity, |node| node.top = px(top));
    }
    let bars: Vec<Entity> = world
        .query_filtered::<Entity, With<ToolbarRoot>>()
        .iter(world)
        .collect();
    // The needs panel ends at 224; a narrow window moves the hotbar right of it.
    let clear = t::SCREEN_MARGIN + NEEDS_WIDTH + t::SCREEN_MARGIN;
    let left = if left < clear && layout.width - total - t::SCREEN_MARGIN >= clear {
        clear
    } else {
        left
    };
    for entity in bars {
        set_node(world, entity, |node| node.left = px(left));
    }
    for (_, entity) in tiles {
        set_node(world, entity, |node| node.width = px(tile));
    }
    let (left, right, bottom) = prompt_lane(layout.width, super::workspace_width(world));
    let lanes: Vec<Entity> = world
        .query_filtered::<Entity, With<PromptRoot>>()
        .iter(world)
        .collect();
    for entity in lanes {
        set_node(world, entity, |node| {
            node.left = px(left);
            node.right = px(right);
            node.bottom = px(bottom);
        });
    }
}

/// What the refresh hands the HUD: read once from the observation.
pub(super) struct Facts<'a> {
    pub(super) values: [u64; 5],
    pub(super) coins: u64,
    pub(super) journey: &'static str,
    pub(super) goal: String,
    pub(super) detail: String,
    pub(super) progress: Option<(u32, u32, &'static str)>,
    pub(super) goal_action: Action,
    pub(super) prompt: Prompt,
    pub(super) context: Option<Action>,
    pub(super) feedback: Option<&'a str>,
    pub(super) open: Option<Tab>,
    pub(super) folded: bool,
    /// The goal chain of live mode, where the world has one.
    pub(super) chain: Option<super::depth::Chain>,
    /// Why each need is low, in the simulation's words, in `NEEDS` order.
    pub(super) reasons: [Option<String>; 5],
    /// The share of the current work done, where its length is known.
    pub(super) work: Option<f32>,
    /// A repeat order: what it repeats and "3 of 10".
    pub(super) repeat: Option<(String, String)>,
}

pub(super) fn refresh(world: &mut World, facts: Facts<'_>) {
    let hud = world.resource::<Hud>();
    let (needs, coins, goal, prompt, tiles) = (
        hud.needs.clone(),
        hud.coins,
        hud.goal,
        hud.prompt,
        hud.tiles.clone(),
    );
    for (((key, row, bar), value), reason) in needs
        .into_iter()
        .zip(facts.values)
        .zip(facts.reasons.iter())
    {
        let status = need_status(value);
        // A critical need pulses (see `pulse`); the others are still.
        let critical = status == Status::Crit;
        if critical != world.get::<CriticalNeed>(row).is_some() {
            if critical {
                world.entity_mut(row).insert(CriticalNeed);
            } else {
                world.entity_mut(row).remove::<(CriticalNeed, Outline)>();
            }
        }
        w::set_need(
            world,
            bar,
            value.min(100_000) as f32 / 100_000.0,
            status,
            Surface::Chrome,
        );
        let (_, name, picture) = NEEDS
            .into_iter()
            .find(|(candidate, ..)| *candidate == key)
            .expect("a listed need");
        // The simulation's reason comes first when there is one.
        let body = match reason {
            Some(reason) => format!("{} {reason}", need_sentence(name, value)),
            None => need_sentence(name, value),
        };
        let click = if key == "energy" {
            "Click: open your pouch at the ways to rest."
        } else {
            "Click: open your pouch at what helps."
        };
        tip(
            world,
            row,
            Tooltip::new(name, body)
                .icon(picture)
                .value(format!("{}%", (value / 1000).min(100)))
                .detail([click]),
        );
    }
    if let Some(coins) = coins {
        w::set_text(world, coins, &facts.coins.to_string());
    }
    for (tab, entity) in tiles {
        w::set_state(world, entity, tab.is_some() && tab == facts.open, false);
    }
    if let Some(goal) = goal {
        // One line while a workspace is open, or while the player folded it.
        let folded = facts.folded || facts.open.is_some();
        w::set_text(world, goal.caption, facts.journey);
        w::set_text(world, goal.title, &facts.goal);
        w::set_text(world, goal.detail, &facts.detail);
        // The hint under the goal goes with the tips setting; the goal stays.
        let hints = crate::island_shell::coach::hints_wanted(world);
        w::show(
            world,
            goal.detail,
            hints && !folded && !facts.detail.is_empty(),
        );
        w::show(world, goal.progress, !folded && facts.progress.is_some());
        if let Some((current, target, unit)) = facts.progress {
            w::set_bar(
                world,
                goal.bar,
                current as f32 / target.max(1) as f32,
                Some(&format!("{current} / {target} {unit}")),
            );
        }
        match &facts.chain {
            Some(chain) => {
                w::set_text(world, goal.step, &chain_words(chain));
                let current = (chain.done < chain.total).then_some(chain.done);
                w::set_pips(
                    world,
                    goal.pips,
                    Surface::Chrome,
                    chain.done,
                    current,
                    chain.total,
                );
                w::set_text(world, goal.reward, &chain_reward(chain));
            }
            None => w::set_text(world, goal.step, ""),
        }
        w::show(world, goal.pips, !folded && facts.chain.is_some());
        w::show(
            world,
            goal.reward,
            !folded
                && facts
                    .chain
                    .as_ref()
                    .is_some_and(|chain| !chain_reward(chain).is_empty()),
        );
        // A goal of the chain done: said once, with what comes next.
        let done = facts.chain.as_ref().map(|chain| chain.done);
        let before = world.resource::<Hud>().goals_done;
        if let (Some(before), Some(now), Some(chain)) = (before, done, facts.chain.as_ref())
            && now > before
            && let Some(title) = &chain.last_done
        {
            let next = chain
                .current
                .as_ref()
                .map_or_else(String::new, |next| format!(" Next: {next}."));
            crate::island_shell::notice(world, format!("Goal done: {title}.{next}"));
        }
        world.resource_mut::<Hud>().goals_done = done;
        w::show(world, goal.button, !folded);
        let label = goal_button(&facts.goal_action);
        let texts: Vec<Entity> = world
            .get::<Children>(goal.button)
            .map(|children| children.to_vec())
            .unwrap_or_default()
            .into_iter()
            .filter(|child| world.get::<Text>(*child).is_some())
            .collect();
        for text in texts {
            w::set_text(world, text, &label);
        }
        if world.resource::<Hud>().folded_icon != Some(facts.folded) {
            world.entity_mut(goal.fold_icon).despawn_children();
            let picture = icon::icon(
                world,
                goal.fold_icon,
                if facts.folded {
                    Icon::Collapsed
                } else {
                    Icon::Expanded
                },
                10.0,
                Tone::secondary(Surface::Chrome),
            );
            world.entity_mut(picture).insert(w::Part::Secondary);
            world.resource_mut::<Hud>().folded_icon = Some(facts.folded);
        }
    }
    if let Some(parts) = prompt {
        // A repeat order is work under way even between two gatherings.
        let prompt = match (&facts.prompt, &facts.repeat) {
            (Prompt::None | Prompt::Reach { .. }, Some((what, _))) => {
                Prompt::Working { text: what.clone() }
            }
            (prompt, _) => prompt.clone(),
        };
        let fraction = matches!(prompt, Prompt::Working { .. })
            .then_some(facts.work)
            .flatten();
        w::show(world, parts.working_bar.entity, fraction.is_some());
        if let Some(fraction) = fraction {
            w::set_bar(world, parts.working_bar, fraction, None);
        }
        let count = facts
            .repeat
            .as_ref()
            .map_or_else(String::new, |(_, count)| count.clone());
        w::set_text(world, parts.working_count, &count);
        w::show(world, parts.working_count, !count.is_empty());
        let (reach, working) = match &prompt {
            Prompt::None => (false, false),
            Prompt::Reach { text, enabled } => {
                w::set_text(world, parts.reach_text, text);
                w::set_enabled(world, parts.reach, *enabled);
                if let Some(action) = facts.context.clone()
                    && world.get::<PlayerAction>(parts.reach).map(|held| &held.0) != Some(&action)
                {
                    world.entity_mut(parts.reach).insert(PlayerAction(action));
                }
                (true, false)
            }
            Prompt::Working { text } => {
                w::set_text(world, parts.working_text, text);
                (false, true)
            }
        };
        w::show(world, parts.reach, reach);
        w::show(world, parts.working, working);
        let lanes: Vec<Entity> = world
            .query_filtered::<Entity, With<PromptRoot>>()
            .iter(world)
            .collect();
        for lane in lanes {
            w::show(world, lane, reach || working);
        }
    }
    // An outcome is said once, as a toast under the bar; it is not a
    // permanent line of the HUD.
    let fresh = facts.feedback.map(str::to_owned);
    if fresh != world.resource::<Hud>().feedback {
        if let Some(text) = &fresh {
            crate::island_shell::notice(world, text.clone());
        }
        world.resource_mut::<Hud>().feedback = fresh;
    }
}
