//! Sleep and the morning after. While the played character sleeps the view's
//! edges dim and a strip at the prompt's place says until when, with Wake up
//! (the host runs the clock faster meanwhile; the simulation skips no tick). On waking from a night's sleep
//! the day's account opens as a card on parchment; Enter or Esc puts it away.
use super::*;
use crate::game_ui::icon::{self, Icon};
use crate::game_ui::tokens::{self as t, Surface};
use crate::game_ui::tooltip::Tooltip;
use crate::game_ui::widgets::{self as w, ButtonSpec, Kind, Reach, Tone};
use bevy::picking::Pickable;
use island_content::village::{live::DaySummary, view};

#[derive(Component)]
pub(crate) struct VeilRoot;
#[derive(Component)]
pub(crate) struct SummaryRoot;
#[derive(Component)]
struct SummaryHeading;
#[derive(Component)]
struct SummaryBody;
#[derive(Component)]
struct SummaryButton;
/// The card is 460 wide with 20 of padding a side; a label takes 84 and a
/// gap 10 of the rest, less a little slack for the edge.
const SUMMARY_TEXT_WIDTH: f32 = 460.0 - 2.0 * 20.0 - 84.0 - 10.0 - 8.0;

/// "morning at 06:00" on the sleeping strip.
#[derive(Component)]
struct MorningText;

/// The strip's distance from the bottom: where the prompt stands.
const STRIP_BOTTOM: f32 = t::SCREEN_MARGIN + t::DOCK + 16.0;

/// The strip's words for the morning: "· morning at 06:00".
pub(super) fn morning_words(until: u64, day_length: u64) -> String {
    let day = day_length.max(1);
    let permille = (until % day * 1000 / day) as i32;
    format!(
        "\u{b7} morning at {}",
        crate::resident_inspector::model::clock_text(permille)
    )
}

/// What the rest view knows: asleep last frame, the day whose account is
/// open, and the last day whose account was shown.
#[derive(Resource, Default)]
pub(super) struct Rest {
    asleep: bool,
    open: Option<DaySummary>,
    shown_day: Option<u64>,
    focus: bool,
    /// The last season that had ended when the character fell asleep: a
    /// season that ends in the night is told in the morning (2.6).
    season_at_sleep: Option<u32>,
}

/// The morning account's line after the season's last night: "Autumn has
/// ended. The island has 43 residents; 2 buildings were finished."
pub(super) fn season_line(name: &str, residents: u32, built: u32) -> String {
    let built = match built {
        0 => "no building was finished".to_owned(),
        1 => "1 building was finished".to_owned(),
        built => format!("{built} buildings were finished"),
    };
    format!("{name} has ended. The island has {residents} residents; {built}.")
}

/// Whether the played character is asleep, by the simulation's account.
pub(crate) fn asleep(world: &World) -> bool {
    depth::played(world).is_some_and(|(unit, _)| view::asleep_until(world, unit).is_some())
}

/// Whether the day's account is open (it takes Enter and Esc).
pub(super) fn summary_open(world: &World) -> bool {
    world
        .get_resource::<Rest>()
        .is_some_and(|rest| rest.open.is_some())
}

pub(super) fn dismiss(world: &mut World) {
    if let Some(mut rest) = world.get_resource_mut::<Rest>() {
        rest.open = None;
    }
    world.resource_mut::<InputFocus>().clear();
}

fn full_screen(display: Display) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: px(0),
        right: px(0),
        top: px(0),
        bottom: px(0),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        display,
        ..default()
    }
}

pub(super) fn spawn(world: &mut World) {
    world.init_resource::<Rest>();
    // Sleep dims the edges of the view and leaves its middle clear: the
    // sleeper lies in the cutaway cottage there. The veil sits under the top
    // bar and the HUD's chrome, so the clock runs visibly through the night.
    // One strip says so where the prompt stands. Nothing moves (Reduce
    // motion has nothing to stop).
    let veil = world
        .spawn((
            Name::new("Sleeping veil"),
            VeilRoot,
            TabGroup::new(8),
            GlobalZIndex(t::layer::CHROME - 1),
            Pickable::IGNORE,
            full_screen(Display::None),
            BackgroundGradient(vec![Gradient::Radial(RadialGradient::new(
                UiPosition::CENTER,
                RadialGradientShape::FarthestCorner,
                vec![
                    ColorStop::percent(t::BARK_900.with_alpha(0.0), 0.0),
                    ColorStop::percent(t::BARK_900.with_alpha(0.0), 38.0),
                    ColorStop::percent(t::BARK_900.with_alpha(0.55), 78.0),
                    ColorStop::percent(t::BARK_900.with_alpha(0.8), 100.0),
                ],
            ))]),
        ))
        .id();
    if let Some(mut node) = world.get_mut::<Node>(veil) {
        node.align_items = AlignItems::FlexEnd;
    }
    let lane = world
        .spawn((
            ChildOf(veil),
            Pickable::IGNORE,
            Node {
                width: percent(100),
                justify_content: JustifyContent::Center,
                margin: UiRect::bottom(px(STRIP_BOTTOM)),
                ..default()
            },
        ))
        .id();
    let strip = world
        .spawn((
            ChildOf(lane),
            Name::new("Sleeping strip"),
            w::chrome(
                Node {
                    padding: UiRect::axes(px(12), px(6)),
                    column_gap: px(10),
                    align_items: AlignItems::Center,
                    ..default()
                },
                t::RADIUS_LG,
            ),
        ))
        .id();
    icon::icon(
        world,
        strip,
        Icon::Rest,
        16.0,
        Tone::secondary(Surface::Chrome),
    );
    w::label(
        world,
        strip,
        "Sleeping\u{2026}",
        t::BODY,
        Tone::text(Surface::Chrome),
    );
    let morning = w::label(world, strip, "", t::DENSE, Tone::secondary(Surface::Chrome));
    world.entity_mut(morning).insert(MorningText);
    let wake = w::button(
        world,
        strip,
        ButtonSpec::new("Wake up", Kind::Quiet, Surface::Chrome)
            .key("E")
            .reach(Reach::Tab(0)),
        (
            PlayerControl,
            PlayerAction(Action::Live(depth::LiveRequest::new("wake"))),
        ),
    );
    world.entity_mut(wake).insert(Tooltip::new(
        "Wake up",
        "Get up before morning. Energy restores less than a full night.",
    ));

    // The day's account: one card on parchment over a scrim.
    let root = world
        .spawn((
            Name::new("Day summary"),
            SummaryRoot,
            TabGroup::new(8),
            Hovered::default(),
            GlobalZIndex(t::layer::SCRIM),
            full_screen(Display::None),
            BackgroundColor(t::SCRIM),
        ))
        .id();
    let panel = w::modal_panel(world, root, "A new morning", "");
    if let Some(mut node) = world.get_mut::<Node>(panel) {
        node.width = px(460);
        node.max_height = percent(92);
    }
    let heading = w::label(
        world,
        panel,
        "",
        t::SUBHEADING,
        Tone::secondary(Surface::Paper),
    );
    world.entity_mut(heading).insert(SummaryHeading);
    world.spawn((
        ChildOf(panel),
        SummaryBody,
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: px(6),
            ..default()
        },
    ));
    let buttons = world
        .spawn((
            ChildOf(panel),
            Node {
                justify_content: JustifyContent::FlexEnd,
                margin: UiRect::top(px(6)),
                ..default()
            },
        ))
        .id();
    let button = w::button(
        world,
        buttons,
        ButtonSpec::new("Good morning", Kind::Primary, Surface::Paper)
            .key("Enter")
            .reach(Reach::Tab(0)),
        (PlayerControl, PlayerAction(Action::DismissSummary)),
    );
    // No tooltip: the button has keyboard focus at once, and its card
    // stood over the account's last rows.
    world.entity_mut(button).insert((
        SummaryButton,
        // Putting the day's account away confirms it.
        crate::game_ui::sound::ControlSound(Some(crate::game_ui::sound::UiSound::Confirm)),
    ));
}

/// The heading of an account: "Day 6 · Autumn".
fn heading(world: &World, day: u64) -> String {
    if !world.contains_resource::<npc_framework::prelude::SimClock>() {
        return format!("Day {day}");
    }
    let season = view::season_effects(world);
    // In the top bar's words ("Autumn, day 6"). Yesterday is in today's
    // season unless today opened a season.
    if season.day == day {
        format!("{:?}, day {}", season.season, season.day_of_season)
    } else if season.day == day + 1 && season.day_of_season > 1 {
        format!("{:?}, day {}", season.season, season.day_of_season - 1)
    } else {
        "Yesterday".to_owned()
    }
}

fn fill(world: &mut World, summary: &DaySummary) {
    let next = depth::chain(world).and_then(|chain| chain.current);
    let events = depth::day_events(world, summary.day);
    let mut lines = depth::summary_lines(
        summary,
        &|id| depth::name_of(world, id),
        next.as_deref(),
        &events,
    );
    // A season that ended in the night.
    let ended = crate::colony_panel::last_season(world);
    if ended.is_some()
        && ended != world.resource::<Rest>().season_at_sleep
        && let Some(season) = ended.and_then(|season| view::season_summary(world, season))
    {
        let built: u32 = season.built.iter().map(|(_, count)| *count).sum();
        lines.push((
            "Season",
            season_line(&season.name, season.population_end, built),
        ));
    }
    let title = heading(world, summary.day);
    let headings: Vec<Entity> = world
        .query_filtered::<Entity, With<SummaryHeading>>()
        .iter(world)
        .collect();
    for entity in headings {
        w::set_text(world, entity, &title);
    }
    let Some(body) = world
        .query_filtered::<Entity, With<SummaryBody>>()
        .iter(world)
        .next()
    else {
        return;
    };
    world.entity_mut(body).despawn_children();
    for (label, text) in lines {
        let line = world
            .spawn((
                ChildOf(body),
                Node {
                    width: percent(100),
                    column_gap: px(10),
                    align_items: AlignItems::FlexStart,
                    ..default()
                },
            ))
            .id();
        let name = w::label(world, line, label, t::DENSE, Tone::caption(Surface::Paper));
        world.entity_mut(name).insert(Node {
            width: px(84),
            flex_shrink: 0.0,
            ..default()
        });
        let goods = match label {
            "Gathered" => Some(&summary.gathered),
            "Made" => Some(&summary.made),
            "Received" => Some(&summary.received),
            _ => None,
        };
        if let Some(goods) = goods {
            // Goods as chips with their pictures.
            let chips = world
                .spawn((
                    ChildOf(line),
                    Node {
                        width: px(SUMMARY_TEXT_WIDTH),
                        flex_shrink: 0.0,
                        flex_wrap: FlexWrap::Wrap,
                        column_gap: px(8),
                        row_gap: px(4),
                        ..default()
                    },
                ))
                .id();
            for (kind, count) in goods {
                let chip = world
                    .spawn((
                        ChildOf(chips),
                        Node {
                            align_items: AlignItems::Center,
                            column_gap: px(4),
                            ..default()
                        },
                    ))
                    .id();
                if let Some(picture) = Icon::of_good(kind) {
                    icon::icon(world, chip, picture, 14.0, Tone::text(Surface::Paper));
                }
                w::label(
                    world,
                    chip,
                    crate::game_ui::words::count(u64::from(*count), kind),
                    t::BODY,
                    Tone::text(Surface::Paper),
                );
            }
        } else {
            let words = w::paragraph(world, line, text, t::BODY, Tone::text(Surface::Paper));
            // A definite width: a long line ("Food 51% · Water 27% · …")
            // wraps inside the card instead of running out of it.
            if let Some(mut node) = world.get_mut::<Node>(words) {
                node.width = px(SUMMARY_TEXT_WIDTH);
                node.flex_shrink = 0.0;
            }
        }
    }
}

/// Show the veil while asleep; open the day's account on waking from a
/// night's sleep. `active`: Live is on screen and no menu is open.
pub(super) fn refresh(world: &mut World, active: bool) {
    let asleep = active && asleep(world);
    let woke = {
        let mut rest = world.resource_mut::<Rest>();
        let woke = rest.asleep && !asleep;
        let fell = !rest.asleep && asleep;
        rest.asleep = asleep;
        (woke, fell)
    };
    let (woke, fell) = woke;
    if fell {
        let season = crate::colony_panel::last_season(world);
        world.resource_mut::<Rest>().season_at_sleep = season;
    }
    if woke
        && let Some((unit, _)) = depth::played(world)
        && let Some(summary) = view::day_summary(world, unit)
        && summary.slept
        && world.resource::<Rest>().shown_day != Some(summary.day)
    {
        fill(world, &summary);
        let mut rest = world.resource_mut::<Rest>();
        rest.shown_day = Some(summary.day);
        rest.open = Some(summary);
        rest.focus = true;
    }
    if asleep
        && let Some((unit, _)) = depth::played(world)
        && let Some(until) = view::asleep_until(world, unit)
    {
        let day = world
            .get_resource::<npc_framework::prelude::SimConfig>()
            .map_or(2_400, |config| config.day_length);
        let words = morning_words(until, day);
        let texts: Vec<Entity> = world
            .query_filtered::<Entity, With<MorningText>>()
            .iter(world)
            .collect();
        for entity in texts {
            w::set_text(world, entity, &words);
        }
    }
    let open = active && world.resource::<Rest>().open.is_some();
    for (mut node, veil) in world
        .query_filtered::<(&mut Node, Has<VeilRoot>), Or<(With<VeilRoot>, With<SummaryRoot>)>>()
        .iter_mut(world)
    {
        let shown = if veil { asleep && !open } else { open };
        let display = if shown { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
    }
    if open && std::mem::take(&mut world.resource_mut::<Rest>().focus) {
        let button = world
            .query_filtered::<Entity, With<SummaryButton>>()
            .iter(world)
            .next();
        if let Some(button) = button {
            world
                .resource_mut::<InputFocus>()
                .set(button, FocusCause::Navigated);
        }
    }
}

#[cfg(test)]
pub(super) fn open_for_test(world: &mut World, summary: DaySummary) {
    fill(world, &summary);
    let mut rest = world.resource_mut::<Rest>();
    rest.shown_day = Some(summary.day);
    rest.open = Some(summary);
}
