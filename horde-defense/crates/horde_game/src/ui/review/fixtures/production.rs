//! The production pass's wave-1 review scenes (design-qa-captures 1.8).
//!
//! Every recipe stages feature state through `Game::apply(&Command)`; the
//! `staging()` seam and the other non-command `Game` mutators (`deploy`,
//! `set_paused`, `sell_item`, ...) appear only inside top-level `entitle_*`
//! helpers (the start country, coins, experience, earlier stage clears and
//! inventory grants); `repo_checks::production_fixtures_stage_features_through_commands`
//! enforces it. Ticking the simulation is play, not staging. Each recipe
//! leaves the game paused on its screen and returns before the matrix's
//! shared progression.
use crate::runner::{RuntimeNotice, SaveIntent};
use crate::ui::{FrontendParts, Screen, Tab};
use horde_core::{
    campaign::{AutoSellRule, NodePayment, Player},
    command::{Command, CommandOutcome},
    content::{Mode, Slot, StructureKind, content},
    defenders::DefenderId,
    quality::Quality,
    sim::{Actor, Input, Outcome, SimEvent},
    skill_catalog::{CampaignFeature, SkillEffect, skills},
    weapons::{WeaponClass, WeaponSpec},
    world::{Country, StageTheme, world},
};

/// Each weapon-class scene and the tier-1 catalogue rifle, pistol, ... it
/// equips (content items 21-26, by their stable ids).
pub(in crate::ui::review) const WEAPON_CLASS_SCENES: [(&str, WeaponClass, &str); 6] = [
    (
        "weapon-classes-pistol",
        WeaponClass::Pistol,
        "compact_service_pistol",
    ),
    ("weapon-classes-smg", WeaponClass::Smg, "compact_smg"),
    (
        "weapon-classes-shotgun",
        WeaponClass::Shotgun,
        "pump_shotgun",
    ),
    (
        "weapon-classes-rifle",
        WeaponClass::Rifle,
        "patrol_rifle_tier1",
    ),
    (
        "weapon-classes-marksman",
        WeaponClass::Marksman,
        "scout_marksman_rifle",
    ),
    (
        "weapon-classes-machine-gun",
        WeaponClass::MachineGun,
        "belt_fed_machine_gun",
    ),
];

/// The battle-mid field. The plan names 7 lanes x 20 yd, which balance.json
/// never deploys (lanes and width grow in the same level steps), so the scene
/// records the nearest reachable geometry with the same lane count.
pub(in crate::ui::review) const BATTLE_MID_GEOMETRY: (u8, f32) = (7, 16.0);
/// How long the battle-mid stage runs unanswered before its still: twenty
/// seconds. Two enemies a lane only gather near defeat.
pub(in crate::ui::review) const BATTLE_MID_TICKS: u64 = 60 * 20;

/// A results-defeat run that has not settled after 10 minutes is a bug.
const DEFEAT_TICK_CAP: u32 = 36_000;
/// The first player shot comes within a second of the trigger.
const SHOT_TICK_CAP: u32 = 60;
/// The first turret shot comes within the stage's first ten seconds.
const TURRET_SHOT_TICK_CAP: u32 = 600;

/// Stages a wave-1 scene; false when `screen` is not one of them.
pub(in crate::ui::review) fn prepare(f: &mut FrontendParts, screen: &str) -> bool {
    match screen {
        "results-defeat" => prepare_results_defeat(f),
        "save-error" => prepare_save_error(f),
        "night-checkpoint" => prepare_night_checkpoint(f),
        "weapon-classes-turret" => prepare_turret(f),
        "battle-mid" => prepare_battle_mid(f),
        "operations-observer" => prepare_operations_observer(f),
        _ => {
            let Some(&(_, class, item)) = WEAPON_CLASS_SCENES
                .iter()
                .find(|(scene, _, _)| *scene == screen)
            else {
                return false;
            };
            prepare_weapon_class(f, class, item);
        }
    }
    true
}

/// The first `(country id, stage)` in world file order, stages `0..level_count`,
/// whose stage theme satisfies `pred`.
pub(in crate::ui::review) fn first_stage_where(
    pred: impl Fn(&Country, usize, &StageTheme) -> bool,
) -> Option<(String, usize)> {
    world().countries.iter().find_map(|country| {
        (0..country.level_count())
            .find(|&stage| pred(country, stage, &country.stage_theme(stage)))
            .map(|stage| (country.id.clone(), stage))
    })
}

// ----- entitlements: the only staging in this file -----

/// The start choice without `Command::ChooseStart`'s own stage-0 deployment.
fn entitle_start(f: &mut FrontendParts, country: &str) {
    f.game
        .staging()
        .campaign()
        .choose_start_country(country)
        .expect("review start country");
    f.campaign.selected_country = country.into();
}
/// Explicit clears of the active country's stages before `stage`.
fn entitle_clears(f: &mut FrontendParts, stage: usize) {
    for clear in 0..stage {
        f.game
            .staging()
            .campaign()
            .record_country_stage_clear(clear)
            .expect("explicit earlier-stage review entitlement");
    }
}
/// Explicit first clears of catalogue levels until `victories` are won.
fn entitle_victories(f: &mut FrontendParts, victories: u64) {
    while f.game.campaign().victories < victories {
        let campaign = f.game.staging().campaign();
        let level = (campaign.victories as usize) % content().levels.len();
        campaign.first_clear(level, Mode::Personal);
        campaign.finish_attempt();
    }
}
fn entitle_coins(f: &mut FrontendParts, coins: u64) {
    f.game.staging().campaign().reward(coins, 0, Mode::Personal);
}
/// Experience through the core's own level-up rules.
fn entitle_xp(f: &mut FrontendParts, xp: u64) {
    f.game.staging().campaign().reward(0, xp, Mode::Personal);
}
/// One level-1 copy of the catalogue item `id`; returns the owned item id.
fn entitle_item(f: &mut FrontendParts, id: &str) -> u64 {
    let definition = content()
        .items
        .iter()
        .position(|item| item.id.as_str() == id)
        .unwrap_or_else(|| panic!("catalogue item {id}"));
    entitle_found(f, definition, 1, None)
}
/// One owned copy of catalogue item `definition` at `level` and `quality`
/// (None is shop stock); returns the owned item id.
fn entitle_found(
    f: &mut FrontendParts,
    definition: usize,
    level: u32,
    quality: Option<Quality>,
) -> u64 {
    f.game
        .staging()
        .campaign()
        .add_item_with_quality(definition, level, quality)
        .expect("explicit review inventory grant")
}

// ----- commands -----

fn command(f: &mut FrontendParts, command: Command) -> CommandOutcome {
    f.game
        .apply(&command)
        .unwrap_or_else(|error| panic!("review fixture {command:?}: {error}"))
}
/// Buys skill node `id` through `Command::BuySkillNode`, prerequisites first,
/// each paid with exactly its entitled price.
fn learn(f: &mut FrontendParts, id: &str) {
    if f.game.campaign().skill_nodes.contains(id) {
        return;
    }
    let node = skills()
        .node(id)
        .unwrap_or_else(|| panic!("skill node {id}"));
    for prerequisite in &node.prerequisites {
        learn(f, prerequisite);
    }
    entitle_victories(f, node.minimum_victories);
    if let Ok(NodePayment::Coins(price)) = f.game.campaign().node_payment(id) {
        entitle_coins(f, price);
    }
    command(f, Command::BuySkillNode(id.into()));
}
/// Deploys `stage` of the active country and leaves the battle paused, with
/// the controls a fresh deployment resets to.
fn deploy(f: &mut FrontendParts, stage: usize, mode: Mode) {
    command(f, Command::Deploy { stage, mode });
    f.screen = Screen::Battle;
    f.campaign.stage = stage;
    f.campaign.mode = mode;
    let a = f.game.attempt().expect("deployed");
    f.battle.aim = [a.width_yards * 0.5, a.defender_position()[1]];
}
fn pause(f: &mut FrontendParts) {
    command(f, Command::SetPaused(true));
}
/// Ticks `input` until a drained event satisfies `found` or `cap` ticks pass.
fn tick_until(
    f: &mut FrontendParts,
    cap: u32,
    input: Input,
    found: impl Fn(&SimEvent) -> bool,
) -> bool {
    // Deployment events are not the ones looked for.
    f.game.drain_events().for_each(drop);
    for _ in 0..cap {
        f.game.tick(input.clone());
        if f.game.drain_events().any(|record| found(&record.event)) {
            return true;
        }
    }
    false
}

// ----- recipes -----

/// A real stage-0 defense nobody fights: it settles as a defeat.
fn prepare_results_defeat(f: &mut FrontendParts) {
    entitle_start(f, "DEU");
    deploy(f, 0, Mode::Personal);
    for _ in 0..DEFEAT_TICK_CAP {
        if f.game.attempt().is_none() {
            break;
        }
        f.game.tick(Input::default());
    }
    assert!(
        f.game.attempt().is_none(),
        "no settlement within {DEFEAT_TICK_CAP} ticks"
    );
    let outcome = f.game.last_result().expect("settled report").outcome;
    assert_eq!(outcome, Outcome::Defeat, "an unfought stage is lost");
    pause(f);
    f.act(|c| c.focus_next_upgrade());
    f.screen = Screen::Results;
}

/// The world camp after a manual save failed: the notice the runner sends,
/// without any save being attempted.
fn prepare_save_error(f: &mut FrontendParts) {
    pause(f);
    f.screen = Screen::Camp;
    f.nav.tab = Tab::World;
    f.notices.push(RuntimeNotice::Saved {
        intent: SaveIntent::Manual,
        result: Err("Access is denied. (os error 5)".into()),
    });
    f.apply_notices();
}

/// The first night checkpoint stage of the world, deployed paused. The theme
/// rule is the core's; only the search is the fixture's.
fn prepare_night_checkpoint(f: &mut FrontendParts) {
    let (country, stage) =
        first_stage_where(|_, _, theme| theme.lighting == "night" && theme.id == "checkpoint")
            .expect("world.json has a night checkpoint stage");
    entitle_start(f, &country);
    entitle_clears(f, stage);
    deploy(f, stage, Mode::Personal);
    pause(f);
}

/// The class's tier-1 weapon, equipped and fired at mid-field once. Firing
/// stays latched, so a live motion capture keeps shooting.
fn prepare_weapon_class(f: &mut FrontendParts, class: WeaponClass, item: &str) {
    entitle_start(f, "DEU");
    // The class proficiency, bought in the skill tree.
    let training = skills()
        .nodes
        .iter()
        .find(|node| {
            node.effects
                .iter()
                .any(|effect| matches!(effect, SkillEffect::WeaponTraining(c) if *c == class))
        })
        .map(|node| node.id.clone())
        .unwrap_or_else(|| panic!("a {class:?} training node"));
    learn(f, &training);
    let id = entitle_item(f, item);
    command(
        f,
        Command::Equip {
            mode: Mode::Personal,
            id,
        },
    );
    let equipped = f.game.campaign().equipped(Mode::Personal, Slot::Ranged);
    assert_eq!(WeaponSpec::from_item(equipped).class, class, "{item}");
    deploy(f, 0, Mode::Personal);
    let input = Input {
        fire: true,
        aim: f.battle.aim,
        ..Input::default()
    };
    let fired = tick_until(f, SHOT_TICK_CAP, input, |event| {
        matches!(
            event,
            SimEvent::Shot {
                shooter: Actor::Defender {
                    mode: Mode::Personal,
                    weapon,
                },
                ..
            } if *weapon == class
        )
    });
    assert!(fired, "no {class:?} shot within {SHOT_TICK_CAP} ticks");
    pause(f);
    f.battle.fire_latched = true;
}

/// A turret bought through the skill tree, placed on the centre lane and
/// ticked to its first shot.
fn prepare_turret(f: &mut FrontendParts) {
    entitle_start(f, "DEU");
    learn(f, "turret_blueprint");
    deploy(f, 0, Mode::Personal);
    let lane = f.game.attempt().expect("deployed").lane_count / 2;
    command(
        f,
        Command::Place {
            kind: StructureKind::Turret,
            lane,
            column: 1,
        },
    );
    let fired = tick_until(f, TURRET_SHOT_TICK_CAP, Input::default(), |event| {
        matches!(
            event,
            SimEvent::Shot {
                shooter: Actor::Structure {
                    kind: StructureKind::Turret,
                    ..
                },
                ..
            }
        )
    });
    assert!(fired, "no turret shot within {TURRET_SHOT_TICK_CAP} ticks");
    pause(f);
}

/// The first level whose deployed geometry is `BATTLE_MID_GEOMETRY`, reached
/// through entitled experience, deployed and left unanswered for
/// `BATTLE_MID_TICKS`: a crowd of at least one enemy a lane is on the field
/// and the defender still stands.
fn prepare_battle_mid(f: &mut FrontendParts) {
    entitle_start(f, "DEU");
    let balance = f.game.campaign().balance_settings.clone();
    let level = (1..=Player::level_cap())
        .find(|&level| balance.geometry(level) == BATTLE_MID_GEOMETRY)
        .expect("a level deploys the battle-mid geometry");
    while f.game.campaign().player.level < level {
        let player = &f.game.campaign().player;
        let missing = player.next_level_xp().saturating_sub(player.xp).max(1);
        entitle_xp(f, missing);
    }
    assert_eq!(f.game.campaign().arena_geometry(), BATTLE_MID_GEOMETRY);
    deploy(f, 0, Mode::Personal);
    for _ in 0..BATTLE_MID_TICKS {
        assert!(f.game.attempt().is_some(), "battle-mid stays live");
        f.game.tick(Input::default());
    }
    pause(f);
}

/// The operations roster with its live expedition, observed.
fn prepare_operations_observer(f: &mut FrontendParts) {
    entitle_start(f, "DEU");
    super::prepare_progression(&mut f.game);
    super::prepare_operations(f);
    f.screen = Screen::Camp;
    f.nav.tab = Tab::Operations;
    f.campaign.observed = f
        .game
        .battles()
        .iter()
        .find(|b| !b.settled && b.game().attempt().is_some())
        .map(|b| b.id);
    assert!(
        f.campaign.observed.is_some(),
        "a live expedition to observe"
    );
    pause(f);
}

// ----- wave-2 staging helpers (design-qa-captures, Wave 2) -----
//
// Core-only: they stage the game state of a wave-3 scene and leave the
// screen alone. Wave 3 registers the scenes, which open their screens on top.

/// The equipment-autosell state: one level-4 tier-1 Ranged drop of every
/// quality (as quality-armory stages them), then the rule "sell below
/// Uncommon" stored through `Command::SetAutoSell`. Returns each drop's id
/// with its quality, in `Quality::ALL` order.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "the equipment-autosell scene registers in wave 3")
)]
pub(in crate::ui::review) fn stage_equipment_autosell(
    f: &mut FrontendParts,
) -> Vec<(Quality, u64)> {
    entitle_start(f, "DEU");
    let definition = content()
        .items
        .iter()
        .position(|item| item.slot == Slot::Ranged && item.tier == 1)
        .expect("a tier-1 ranged item");
    // Level 4, as quality-armory stages them.
    let drops = Quality::ALL
        .into_iter()
        .map(|quality| (quality, entitle_found(f, definition, 4, Some(quality))))
        .collect();
    command(
        f,
        Command::SetAutoSell(AutoSellRule {
            enabled: true,
            sell_below: Quality::Uncommon,
            ..AutoSellRule::default()
        }),
    );
    drops
}

/// The briefing-recruit state: Germany liberated through its entitled stage
/// clears, the Recruitment node bought, one recruit taken through
/// `Command::Recruit` and chosen through `Command::SetNextDefender`. Returns
/// the recruit's id.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "the briefing-recruit scene registers in wave 3")
)]
pub(in crate::ui::review) fn stage_briefing_recruit(f: &mut FrontendParts) -> u64 {
    entitle_start(f, "DEU");
    let stages = world().country("DEU").expect("Germany").level_count();
    entitle_clears(f, stages);
    let recruitment = skills()
        .nodes
        .iter()
        .find(|node| {
            node.effects.iter().any(|effect| {
                matches!(
                    effect,
                    SkillEffect::FeatureUnlock(CampaignFeature::Recruitment)
                )
            })
        })
        .map(|node| node.id.clone())
        .expect("a Recruitment node");
    learn(f, &recruitment);
    entitle_coins(f, f.game.campaign().recruit_price());
    let CommandOutcome::Id(id) = command(f, Command::Recruit("DEU".into())) else {
        panic!("Recruit answers the recruit's id");
    };
    command(f, Command::SetNextDefender(DefenderId::Recruit(id)));
    id
}
