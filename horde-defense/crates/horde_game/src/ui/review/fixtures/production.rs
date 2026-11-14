//! The production pass's wave-1 review scenes (design-qa-captures 1.8), the
//! wave-2 close stills (W2-55, W2-57) and the wave-3 UI scenes (W3-27).
//!
//! The recipes in this file stage feature state through
//! `Game::apply(&Command)`; the `staging()` seam and the other non-command
//! `Game` mutators (`deploy`, `set_paused`, `sell_item`, ...) appear here
//! only inside top-level `entitle_*` helpers (the start country, coins,
//! experience, earlier stage clears, inventory grants, the boss-phase
//! clock and wound, the boss-warning clock, the low-HP defender's wound,
//! liberated countries, an outbreak, folded expedition outcomes, a run's
//! drops and the final boss at one hit point; the boss-warning still's
//! banner age is presentation state, not game state);
//! `repo_checks::production_fixtures_stage_features_through_commands`
//! enforces it for this file only. One exception, sanctioned by
//! design-qa-captures (operations-observer row): `prepare_operations_observer`
//! reuses the shared `fixtures::prepare_progression` and
//! `fixtures::prepare_operations` roster, which stage through `staging()`
//! and `deploy_survivor` in `fixtures.rs`, outside the check. Ticking the
//! simulation is play, not staging. Each recipe leaves the game paused on its
//! screen and returns before the matrix's shared progression.
use crate::runner::{RuntimeNotice, SaveIntent};
use crate::ui::{FrontendParts, Screen, Tab};
use horde_core::{
    campaign::{AutoSellRule, NodePayment, Player},
    command::{Command, CommandOutcome},
    content::{Mode, Slot, StructureKind, content},
    defenders::DefenderId,
    enemy_catalog::{ResolvedEnemy, enemy_catalog},
    quality::Quality,
    sim::{Actor, Attempt, DEFENSE_LINE, EnemyPhase, Input, Outcome, SimEvent},
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

/// The boss-phase rank: the first whose Normal boss has a phase (W2-36).
pub(in crate::ui::review) const BOSS_PHASE_RANK: u64 = 5;
/// Smoke-input play before the boss-phase clock advances: twenty seconds.
const BOSS_PHASE_PLAY_TICKS: u32 = 60 * 20;
/// The director spawns the boss within a second of its time.
const BOSS_ARRIVAL_TICK_CAP: u32 = 60;
/// The boss walks in for eight seconds, as in the boss scene.
const BOSS_WALK_TICKS: u32 = 60 * 8;
/// How far into its 2.0 s the boss-warning still holds the banner: past the
/// fade-in and before the fade-out, so both modes draw it at full strength.
const BOSS_WARNING_SHOWN_SECS: f32 = 1.0;
/// The low-HP scenes' defender HP, in percent of its maximum: inside the
/// 30% band, clear of its threshold.
const LOW_HP_SCENE_PCT: i32 = 25;
/// The spitter-windup rank: the first whose wave tier fields the catalogue
/// spitter (rank 6 plays wave tier 5, W2-31).
pub(in crate::ui::review) const SPITTER_WINDUP_RANK: u64 = 6;
/// A rank-6 stage stations its first spitter well inside two minutes.
const SPITTER_STATION_TICK_CAP: u32 = 60 * 120;
/// A results-defeat run that has not settled after 10 minutes is a bug.
const DEFEAT_TICK_CAP: u32 = 36_000;
/// The first player shot comes within a second of the trigger.
const SHOT_TICK_CAP: u32 = 60;
/// The first turret shot comes within the stage's first ten seconds.
const TURRET_SHOT_TICK_CAP: u32 = 600;

/// Stages a production-pass scene; false when `screen` is not one of them.
pub(in crate::ui::review) fn prepare(f: &mut FrontendParts, screen: &str) -> bool {
    match screen {
        "results-defeat" => prepare_results_defeat(f),
        "save-error" => prepare_save_error(f),
        "night-checkpoint" => prepare_night_checkpoint(f),
        "weapon-classes-turret" => prepare_turret(f),
        "battle-mid" => prepare_battle_mid(f),
        "operations-observer" => prepare_operations_observer(f),
        "boss-phase" => prepare_boss_phase(f),
        "boss-warning" => prepare_boss_warning(f),
        "spitter-windup" => prepare_spitter_windup(f),
        "upgrades" => prepare_upgrades(f),
        "camp-home" => prepare_camp_home(f),
        "camp-reports" => prepare_camp_reports(f),
        "low-hp" => prepare_low_hp(f, Mode::Personal),
        "low-hp-observed" => prepare_low_hp(f, Mode::Npc),
        "callout-camp" => prepare_camp_home(f),
        "callout-upgrades" => {
            prepare_camp_home(f);
            f.act(|c| {
                c.focus_next_upgrade();
                c.open_upgrades();
            });
        }
        "briefing-recruit" => prepare_briefing_recruit(f),
        "equipment-autosell" => prepare_equipment_autosell(f),
        "results-liberation-recruit" => prepare_results_liberation_recruit(f),
        "takeover-battle" => prepare_takeover_battle(f),
        "dawn-reference" => prepare_dawn_reference(f),
        "elite-tells" => prepare_elite_tells(f),
        "storm" => prepare_storm(f),
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
/// The defense clock one tick before its boss's time, as
/// `peak_render_fixture` sets it, so the director spawns the boss itself.
fn entitle_boss_clock(f: &mut FrontendParts) {
    let a = f.game.staging().attempt().expect("a live defense");
    let boss_tick = (a.boss_time() * 60.0).ceil() as u64;
    a.tick = a.tick.max(boss_tick.saturating_sub(1));
}
/// The defense clock one tick before its boss warning is due (the boss's
/// tick less the level's warning lead, `Attempt::boss_warning_tick`), so the
/// director sends the warning itself on the next tick.
fn entitle_warning_clock(f: &mut FrontendParts) {
    let a = f.game.staging().attempt().expect("a live defense");
    let boss_tick = (a.boss_time() * 60.0).ceil() as u64;
    let lead = (content().levels[a.level].boss_timing.warning * 60.0).ceil() as u64;
    a.tick = a.tick.max(boss_tick.saturating_sub(lead).saturating_sub(1));
}
/// The boss banner `BOSS_WARNING_SHOWN_SECS` into the warning the paused
/// defense just received: presentation state only (`BattleUi`), so a still
/// draws the banner at full strength where a paused frame adds no time.
fn entitle_banner_shown(f: &mut FrontendParts) {
    let a = f.game.attempt().expect("a live defense");
    let notice = crate::battle_view::BossNotice::of(a);
    let event = notice.event.expect("the director warned");
    f.battle.boss_banner = Some(crate::ui::battle::BossBanner {
        key: (notice.attempt, event.tick),
        arrived: event.arrived,
        shown: BOSS_WARNING_SHOWN_SECS,
        frame: 0,
    });
}
/// The live defender at `percent` of its maximum HP (at least one hit
/// point), as a defense that took that much damage would stand.
fn entitle_defender_hp(f: &mut FrontendParts, percent: i32) {
    let a = f.game.staging().attempt().expect("a live defense");
    a.hp = (a.max_hp * percent / 100).max(1);
}
/// The live boss wounded to half its hit points, its first phase threshold;
/// the next tick enters the phase by the core's own rule.
fn entitle_boss_wound(f: &mut FrontendParts) {
    let a = f.game.staging().attempt().expect("a live defense");
    let boss = a.enemies.iter_mut().find(|e| e.boss).expect("the boss");
    boss.hp = boss.max_hp / 2;
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
/// Countries marked liberated, as `fixtures::prepare_operations` marks them:
/// every stage cleared.
fn entitle_liberated(f: &mut FrontendParts, countries: &[&str]) {
    for id in countries {
        let stages = world().country(id).expect("a world country").level_count();
        let campaign = f.game.staging().campaign();
        if !campaign.world.cleared.iter().any(|c| c == id) {
            campaign.world.cleared.push((*id).into());
        }
        campaign.world.country_levels.insert((*id).into(), stages);
    }
}
/// `count` expedition outcomes folded into the pruning count, as pruning
/// folds an unacknowledged outcome once the battle list is over its cap.
fn entitle_folded_outcomes(f: &mut FrontendParts, count: u32) {
    let strategy = f.game.staging().strategy();
    strategy.folded_outcomes = strategy.folded_outcomes.saturating_add(count);
}
/// An active outbreak in liberated `country`, by the strategy layer's own
/// rule (its strength is the draw the world clock would make).
fn entitle_outbreak(f: &mut FrontendParts, country: &str) {
    let (strategy, campaign) = f.game.staging().strategy_and_campaign();
    strategy
        .trigger_outbreak(campaign, country)
        .expect("explicit review outbreak");
}
/// One drop of catalogue item `definition` at `quality` in the live main
/// run, banked as the drop site banks it: the stash item, the run's drop
/// ids, its loot count and its quality counts.
fn entitle_drop(f: &mut FrontendParts, definition: usize, quality: Quality) -> u64 {
    let id = entitle_found(f, definition, 1, Some(quality));
    let a = f.game.staging().attempt().expect("a live defense");
    a.drop_ids.push(id);
    a.loot += 1;
    a.loot_quality_counts[quality] += 1;
    id
}
/// The live stage's boss at one hit point in mid-field, as
/// `fixtures::prepare_journey_victory` stages it; returns where to aim. The
/// kill and its settlement are the core's.
fn entitle_dying_boss(f: &mut FrontendParts) -> [f32; 2] {
    let mut a = f
        .game
        .staging()
        .attempt_slot()
        .take()
        .expect("a live defense");
    a.boss_spawned = true;
    let lane = a.lane_count / 2;
    f.game
        .spawn_enemy(&mut a, horde_core::content::EnemyKind::Brute, lane, true);
    let target = a.enemies.last_mut().expect("the boss");
    target.x = a.width_yards * 0.55;
    target.hp = 1;
    target.armor = 0;
    let aim = [target.x, target.y()];
    *f.game.staging().attempt_slot() = Some(a);
    aim
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
/// Buys the skill node that unlocks `feature`.
fn learn_feature(f: &mut FrontendParts, feature: CampaignFeature) {
    let node = skills()
        .nodes
        .iter()
        .find(|node| {
            node.effects
                .iter()
                .any(|effect| matches!(effect, SkillEffect::FeatureUnlock(f) if *f == feature))
        })
        .map(|node| node.id.clone())
        .unwrap_or_else(|| panic!("a {feature:?} node"));
    learn(f, &node);
}
/// Takes the first recruit offer of liberated `country` through
/// `Command::Recruit`, paid with exactly its entitled price.
fn recruit(f: &mut FrontendParts, country: &str) -> u64 {
    entitle_coins(f, f.game.campaign().recruit_price());
    let CommandOutcome::Id(id) = command(f, Command::Recruit(country.into())) else {
        panic!("Recruit answers the recruit's id");
    };
    id
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

/// A boss inside its phase hold (W2-37's close capture): a stage-0 defense
/// at `BOSS_PHASE_RANK`, played with the smoke input for
/// `BOSS_PHASE_PLAY_TICKS`. Then the clock advances to the boss's time and
/// the director spawns the boss by its own rule (a level-1 survivor does not
/// outlast a rank-5 horde to its 240-second boss). The boss walks in for
/// `BOSS_WALK_TICKS`, is wounded to its first threshold and enters the phase
/// on the next tick, where the still pauses with the whole hold left; a
/// motion export plays it all (CF-2-03, `review::motion_release_frame`).
fn prepare_boss_phase(f: &mut FrontendParts) {
    entitle_start(f, "DEU");
    entitle_victories(f, BOSS_PHASE_RANK);
    deploy(f, 0, Mode::Personal);
    for _ in 0..BOSS_PHASE_PLAY_TICKS {
        assert!(f.game.attempt().is_some(), "the defense stands");
        let input = horde_core::sim::smoke_input(&f.game);
        f.game.tick(input);
    }
    entitle_boss_clock(f);
    for _ in 0..BOSS_ARRIVAL_TICK_CAP {
        if f.game.attempt().expect("the defense stands").boss_spawned {
            break;
        }
        f.game.tick(Input::default());
    }
    assert!(
        f.game.attempt().is_some_and(|a| a.boss_spawned),
        "no boss within {BOSS_ARRIVAL_TICK_CAP} ticks of its time"
    );
    for _ in 0..BOSS_WALK_TICKS {
        f.game.tick(Input::default());
    }
    entitle_boss_wound(f);
    let entered = tick_until(f, 1, Input::default(), |event| {
        matches!(event, SimEvent::BossPhase { phase: 1, .. })
    });
    assert!(entered, "the wounded boss enters its first phase");
    pause(f);
}

/// The W3-14 boss banner (closing phase): a stage-0 defense after one
/// victory (no first-defense lesson over the field), played with the smoke
/// input for `BOSS_PHASE_PLAY_TICKS`. Then the clock advances to one tick
/// before the boss warning. A motion export pauses there and resumes one
/// frame before frame-0000 (`review::motion_release_frame`), so its frames
/// start on the warning tick, where the banner starts. A still plays the one
/// tick, so the director warns by its own rule, and pauses with the banner
/// `BOSS_WARNING_SHOWN_SECS` into its life.
fn prepare_boss_warning(f: &mut FrontendParts) {
    entitle_start(f, "DEU");
    entitle_victories(f, 1);
    deploy(f, 0, Mode::Personal);
    for _ in 0..BOSS_PHASE_PLAY_TICKS {
        assert!(f.game.attempt().is_some(), "the defense stands");
        let input = horde_core::sim::smoke_input(&f.game);
        f.game.tick(input);
    }
    entitle_warning_clock(f);
    let a = f.game.attempt().expect("the defense stands");
    assert_eq!(a.boss_warning_tick(), None, "the warning is still to come");
    if f.launch.motion_review() && f.launch.sequence_path().is_some() {
        pause(f);
        return;
    }
    let warned = tick_until(f, 1, Input::default(), |event| {
        matches!(event, SimEvent::BossWarning { .. })
    });
    assert!(warned, "the director warns on the next tick");
    pause(f);
    entitle_banner_shown(f);
}

/// The low-HP band (UX-12, W3-15, closing) for the defender `mode` deploys:
/// a stage-0 defense after one victory, the personal one played 20 s with
/// the smoke input and Mara's (`Mode::Npc`, the observed main run) by her
/// own decisions, then the defender entitled to `LOW_HP_SCENE_PCT` of its
/// maximum HP and the defense paused, so both modes draw the band static
/// and the HP text reads "· LOW".
fn prepare_low_hp(f: &mut FrontendParts, mode: Mode) {
    entitle_start(f, "DEU");
    entitle_victories(f, 1);
    if mode == Mode::Npc {
        // Mara joins through the Companion node, which needs a liberated
        // country; Germany stays the front she defends.
        entitle_liberated(f, &[OUTBREAK_COUNTRY]);
        learn_feature(f, CampaignFeature::Companion);
    }
    deploy(f, 0, mode);
    for _ in 0..BOSS_PHASE_PLAY_TICKS {
        assert!(f.game.attempt().is_some(), "the defense stands");
        let input = match mode {
            Mode::Personal => horde_core::sim::smoke_input(&f.game),
            Mode::Npc => Input::default(),
        };
        f.game.tick(input);
    }
    entitle_defender_hp(f, LOW_HP_SCENE_PCT);
    pause(f);
}

/// The id of a catalogue spitter winding up a spit from its station at its
/// full reach beyond the defense line, if one does.
fn spitter_at_reach(a: &Attempt) -> Option<u64> {
    a.enemies
        .iter()
        .find(|e| {
            e.resolved.as_ref().is_some_and(|r| {
                r.archetype_id == "spitter" && (e.x - (DEFENSE_LINE + r.reach)).abs() < 0.01
            }) && e.attack_phase == EnemyPhase::Windup
                && a.spit_target(e).is_some()
        })
        .map(|e| e.id)
}

/// A spitter winding up at its reach (W2-28's close capture, CF-2-04): a
/// stage-0 defense at `SPITTER_WINDUP_RANK`, played with the smoke input
/// (which shoots the nearest hostile, so a stationed spitter outlives the
/// crowd at the line) until the director's first spitter stations four
/// yards beyond the line and winds up. It is then left unanswered to the
/// middle of that windup, where the still pauses. Everything after the rank
/// entitlement is play.
fn prepare_spitter_windup(f: &mut FrontendParts) {
    entitle_start(f, "DEU");
    entitle_victories(f, SPITTER_WINDUP_RANK);
    deploy(f, 0, Mode::Personal);
    let mut spitter = None;
    for _ in 0..SPITTER_STATION_TICK_CAP {
        let a = f.game.attempt().expect("the defense stands");
        spitter = spitter_at_reach(a);
        if spitter.is_some() {
            break;
        }
        let input = horde_core::sim::smoke_input(&f.game);
        f.game.tick(input);
    }
    let spitter = spitter.unwrap_or_else(|| {
        panic!("no spitter winds up at its reach within {SPITTER_STATION_TICK_CAP} ticks")
    });
    loop {
        let a = f.game.attempt().expect("the defense stands");
        let e = a
            .enemies
            .iter()
            .find(|e| e.id == spitter)
            .expect("the spitter lives");
        assert_eq!(e.attack_phase, EnemyPhase::Windup, "the spitter winds up");
        if e.attack_left <= e.windup_total / 2 {
            break;
        }
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
pub(in crate::ui::review) fn stage_briefing_recruit(f: &mut FrontendParts) -> u64 {
    entitle_start(f, "DEU");
    let stages = world().country("DEU").expect("Germany").level_count();
    entitle_clears(f, stages);
    learn_feature(f, CampaignFeature::Recruitment);
    let id = recruit(f, "DEU");
    command(f, Command::SetNextDefender(DefenderId::Recruit(id)));
    id
}

// ----- wave-3 UI scenes (design-qa-captures 1.8 and "Wave 3", W3-27) -----

/// The next front of the wave-3 scenes that need one: Germany's neighbour,
/// chosen through `Command::SelectCountry` (which never deploys).
pub(in crate::ui::review) const SECOND_FRONT: &str = "POL";
/// Where the camp-home expedition fights.
pub(in crate::ui::review) const EXPEDITION_FRONT: &str = "CZE";
/// The liberated country with the camp-home outbreak.
pub(in crate::ui::review) const OUTBREAK_COUNTRY: &str = "FRA";
/// A journey round that has not settled after 5 minutes withdraws.
const UPGRADES_TICK_CAP: u32 = 60 * 300;
/// The camp-home expedition fights for twenty seconds before the still.
const EXPEDITION_TICKS: u32 = 60 * 20;
/// The camp-reports outcomes pruning folded into one count (an entitlement:
/// folding needs more settled expeditions than the battle cap keeps).
pub(in crate::ui::review) const CAMP_REPORTS_FOLDED: u32 = 3;
/// The callout the callout stills leave unread: Expeditions, unlocked in
/// the camp-home state, whose place is Operations, so neither Camp home nor
/// Upgrades marks it seen on arrival.
pub(in crate::ui::review) const UNREAD_CALLOUT: &str = "feature:parallel";

/// The onboarding callout `scene` leaves unread, if it is a callout still
/// (UX-16, closing): the camp-home state on Camp home or on Upgrades, the
/// two places the callout row shows. Every other staged scene marks every
/// unlocked callout seen (`matrix::prepare`).
pub(in crate::ui::review) fn unread_callout(scene: &str) -> Option<&'static str> {
    matches!(scene, "callout-camp" | "callout-upgrades").then_some(UNREAD_CALLOUT)
}

/// The Upgrades screen after a first round: the journey-upgrades round (a real
/// stage-0 defense played with the smoke input, withdrawn if it outlasts
/// `UPGRADES_TICK_CAP`), then the one route to the upgrades,
/// `UiCtx::open_upgrades`, which shows `Screen::Upgrades`.
fn prepare_upgrades(f: &mut FrontendParts) {
    entitle_start(f, "DEU");
    deploy(f, 0, Mode::Personal);
    command(f, Command::SetPaused(false));
    for _ in 0..UPGRADES_TICK_CAP {
        if f.game.attempt().is_none() {
            break;
        }
        let input = horde_core::sim::smoke_input(&f.game);
        f.game.tick(input);
    }
    if f.game.attempt().is_some() {
        command(f, Command::Withdraw);
    }
    pause(f);
    f.screen = Screen::Camp;
    f.act(|c| {
        c.focus_next_upgrade();
        c.open_upgrades();
    });
}

/// Camp home with everything it lists: Mara met, two recruits (one guarding
/// Germany, one on a live expedition), an outbreak in a liberated country
/// and the next front chosen. Orders go through commands; the liberations
/// and the outbreak are entitlements.
fn prepare_camp_home(f: &mut FrontendParts) {
    entitle_start(f, "DEU");
    entitle_liberated(f, &["DEU", OUTBREAK_COUNTRY, "CHE", "AUT"]);
    for feature in [
        CampaignFeature::Companion,
        CampaignFeature::Recruitment,
        CampaignFeature::Parallel,
        CampaignFeature::Operations,
    ] {
        learn_feature(f, feature);
    }
    let guard = recruit(f, "DEU");
    let scout = recruit(f, OUTBREAK_COUNTRY);
    command(
        f,
        Command::AssignGuard {
            id: guard,
            country: "DEU".into(),
        },
    );
    command(
        f,
        Command::DeploySurvivor {
            id: scout,
            country: EXPEDITION_FRONT.into(),
            stage: 0,
        },
    );
    command(f, Command::SetPaused(false));
    for _ in 0..EXPEDITION_TICKS {
        f.game.tick(Input::default());
    }
    entitle_outbreak(f, OUTBREAK_COUNTRY);
    command(f, Command::SelectCountry(SECOND_FRONT.into()));
    pause(f);
    f.campaign.selected_country = SECOND_FRONT.into();
    f.screen = Screen::Camp;
    f.nav.tab = Tab::Home;
}

/// Camp home with expedition outcomes to announce (W3-25, closing): two
/// recruits sent to the expedition front through `Command::DeploySurvivor`
/// fight for `EXPEDITION_TICKS` and are recalled through
/// `Command::RecallSurvivor` (one that settled on its own stays settled), so
/// two settled outcomes wait unacknowledged beside `CAMP_REPORTS_FOLDED`
/// folded ones (an entitlement). The next front is chosen, so the primary is
/// Go to briefing; the first frame toasts all three notices.
fn prepare_camp_reports(f: &mut FrontendParts) {
    entitle_start(f, "DEU");
    entitle_liberated(f, &["DEU", OUTBREAK_COUNTRY, "CHE", "AUT"]);
    for feature in [
        CampaignFeature::Companion,
        CampaignFeature::Recruitment,
        CampaignFeature::Parallel,
    ] {
        learn_feature(f, feature);
    }
    let scouts = [recruit(f, "DEU"), recruit(f, OUTBREAK_COUNTRY)];
    let battles = scouts.map(|id| {
        let CommandOutcome::Id(battle) = command(
            f,
            Command::DeploySurvivor {
                id,
                country: EXPEDITION_FRONT.into(),
                stage: 0,
            },
        ) else {
            panic!("DeploySurvivor answers the expedition's id");
        };
        battle
    });
    command(f, Command::SetPaused(false));
    for _ in 0..EXPEDITION_TICKS {
        f.game.tick(Input::default());
    }
    for battle in battles {
        let live = f
            .game
            .battles()
            .iter()
            .any(|b| b.id == battle && !b.settled);
        if live {
            command(f, Command::RecallSurvivor(battle));
        }
    }
    entitle_folded_outcomes(f, CAMP_REPORTS_FOLDED);
    command(f, Command::SelectCountry(SECOND_FRONT.into()));
    pause(f);
    f.campaign.selected_country = SECOND_FRONT.into();
    f.screen = Screen::Camp;
    f.nav.tab = Tab::Home;
}

/// The briefing of the next front with the recruit chosen to defend it.
fn prepare_briefing_recruit(f: &mut FrontendParts) {
    let recruit = stage_briefing_recruit(f);
    command(f, Command::SelectCountry(SECOND_FRONT.into()));
    assert_eq!(
        f.game.next_defender(),
        DefenderId::Recruit(recruit),
        "choosing the front keeps the chosen defender"
    );
    pause(f);
    f.screen = Screen::Camp;
    f.act(|c| c.open_briefing());
}

/// The Equipment page with the Auto-sell panel open on the staged rule.
fn prepare_equipment_autosell(f: &mut FrontendParts) {
    let drops = stage_equipment_autosell(f);
    pause(f);
    f.screen = Screen::Camp;
    f.nav.tab = Tab::Armory;
    f.armory.sort = crate::ui::armory::ArmorySort::Quality;
    f.armory.selected_item = drops.last().map(|(_, id)| *id);
    f.armory.reveal_auto_sell = true;
}

/// The Results of the victory that liberates Germany, with the auto-sell
/// rule stored before the final win and the Recruitment node bought on the
/// Results screen (the node needs a liberated country, so a first country
/// cannot buy it earlier): the liberation block offers "Recruit from
/// Germany" and the loot block lists what the settlement auto-sold. The
/// final stage is really deployed; one drop of every quality and the boss at
/// one hit point are entitlements, and the kill, the settlement and its
/// sales are the core's.
fn prepare_results_liberation_recruit(f: &mut FrontendParts) {
    entitle_start(f, "DEU");
    command(
        f,
        Command::SetAutoSell(AutoSellRule {
            enabled: true,
            sell_below: Quality::Uncommon,
            keep_upgrades: false,
            ..AutoSellRule::default()
        }),
    );
    let last = world().country("DEU").expect("Germany").level_count() - 1;
    entitle_clears(f, last);
    deploy(f, last, Mode::Personal);
    let definition = content()
        .items
        .iter()
        .position(|item| item.slot == Slot::Ranged && item.tier == 1)
        .expect("a tier-1 ranged item");
    for quality in Quality::ALL {
        entitle_drop(f, definition, quality);
    }
    let aim = entitle_dying_boss(f);
    command(f, Command::SetPaused(false));
    f.game.tick(Input {
        fire: true,
        aim,
        ..Input::default()
    });
    assert!(
        f.game.attempt().is_none(),
        "the boss kill settles the stage"
    );
    let report = f.game.last_result().expect("settled report");
    assert_eq!(report.outcome, Outcome::Victory);
    assert!(report.country_freed, "the last stage liberates Germany");
    pause(f);
    learn_feature(f, CampaignFeature::Recruitment);
    entitle_coins(f, f.game.campaign().recruit_price());
    f.act(|c| c.focus_next_upgrade());
    f.screen = Screen::Results;
}

/// A recruit holding the main defense: chosen through `SetNextDefender`,
/// deployed on the next front through `BeginDefense`, paused before the
/// first tick.
fn prepare_takeover_battle(f: &mut FrontendParts) {
    let recruit = stage_briefing_recruit(f);
    command(f, Command::SelectCountry(SECOND_FRONT.into()));
    command(f, Command::BeginDefense { stage: 0 });
    pause(f);
    let a = f.game.attempt().expect("the takeover deployed");
    assert_eq!(a.defender(), DefenderId::Recruit(recruit));
    f.battle.aim = [a.width_yards * 0.5, a.defender_position()[1]];
    f.campaign.selected_country = SECOND_FRONT.into();
    f.campaign.stage = 0;
    f.campaign.mode = Mode::Npc;
    f.screen = Screen::Battle;
}

// ----- W5-09: the wave-5 graphics scenes (design-vfx-lighting §12, §14) -----

/// The elite-tells rank: the spitter-windup rank, whose stage-0 waves field
/// enough archetypes for every affix to be offered to one.
pub(in crate::ui::review) const ELITE_TELLS_RANK: u64 = SPITTER_WINDUP_RANK;
/// Hostiles for every affix plus a plain one walk in well inside two minutes.
const ELITE_TELLS_TICK_CAP: u32 = 60 * 120;
/// The share of fx-stress hostiles that are elite, in percent.
#[cfg(feature = "review")]
pub(in crate::ui::review) const FX_STRESS_ELITE_PCT: usize = 30;

/// The country and stage of the night-checkpoint scene.
fn night_checkpoint_stage() -> (String, usize) {
    first_stage_where(|_, _, theme| theme.lighting == "night" && theme.id == "checkpoint")
        .expect("world.json has a night checkpoint stage")
}

/// The night-checkpoint scene's dawn pair for the luminance gate: the first
/// dawn stage of the same country, deployed paused the same way, so the same
/// biome and field geometry at tick 0 with no hostile.
fn prepare_dawn_reference(f: &mut FrontendParts) {
    let (country, _) = night_checkpoint_stage();
    let (_, stage) = first_stage_where(|c, _, theme| c.id == country && theme.lighting == "dawn")
        .unwrap_or_else(|| panic!("{country} has a dawn stage"));
    entitle_start(f, &country);
    entitle_clears(f, stage);
    deploy(f, stage, Mode::Personal);
    pause(f);
}

/// Ticks from the staged tick to the pulse's start in a motion export: the
/// 60 native frames before frame-0000 (`capture_sequence::FIRST`, a module
/// only review builds compile) and eight captured frames of four ticks, so
/// the pulse (16 ticks) spans frames 8 to 12 of the 24.
/// `storm_fixture_is_a_storm_tagged_stage` holds it to the export's window.
const STORM_LEAD_TICKS: u64 = 60 + 8 * 4;

/// The start in ms of the pulse the storm scene captures on a stage with
/// lightning seed `seed`: the first one a motion export can lead into.
fn storm_pulse_start(seed: u64) -> u64 {
    crate::art::lighting::first_pulse_after(seed, STORM_LEAD_TICKS * 1000 / 60)
}

/// The first storm stage of the world (ruling W-8: the presentation's storm
/// table, no content field) whose lightning strikes in its first eight
/// seconds, before an unanswered horde reaches the line (an attack cue
/// silences the sky). It is deployed and left unanswered up to the tick
/// from which a motion export captures that pulse whole: the export runs
/// one tick a native frame from the staged tick and reads its lightning
/// clock from the tick. The still is paused there, on the static storm
/// grade (a paused field never flashes). The pulse schedule is the
/// presentation's (`lighting::first_pulse_after`); only the search is the
/// fixture's.
fn prepare_storm(f: &mut FrontendParts) {
    use crate::art::lighting;
    let (country, stage) = first_stage_where(|_, _, theme| {
        lighting::is_storm(theme) && storm_pulse_start(theme.variation) < lighting::SKY_SLOT_MS
    })
    .expect("world.json has a storm stage with an early pulse");
    entitle_start(f, &country);
    entitle_clears(f, stage);
    deploy(f, stage, Mode::Personal);
    let seed = crate::art::scene_look(f.game.attempt().expect("deployed")).seed;
    // The first tick whose clock has reached the pulse's start.
    let start_tick = (storm_pulse_start(seed) * 60).div_ceil(1000);
    for _ in 0..start_tick - STORM_LEAD_TICKS {
        f.game.tick(Input::default());
    }
    assert!(f.game.attempt().is_some(), "the storm stage stays live");
    pause(f);
}

/// For every shipped affix, in catalogue order, a hostile inside the field
/// and the elite profile `elite_profile` gives it for that affix. A hostile
/// that already rolled an affix covers it. None until the field holds a
/// hostile for every affix and one more that stays plain.
fn elite_tell_casting(a: &Attempt) -> Option<Vec<(u64, ResolvedEnemy)>> {
    let catalog = enemy_catalog();
    let rolled = |affix: &str| {
        a.enemies.iter().any(|e| {
            e.resolved
                .as_ref()
                .and_then(|profile| profile.elite.as_ref())
                .is_some_and(|mark| mark.affix_id == affix)
        })
    };
    let mut cast: Vec<(u64, ResolvedEnemy)> = Vec::new();
    for affix in catalog.elite_affix_ids().filter(|affix| !rolled(affix)) {
        let pick = a
            .enemies
            .iter()
            .filter(|e| !e.boss && !e.split_child && e.x < a.width_yards - 1.0)
            .filter(|e| cast.iter().all(|(id, _)| *id != e.id))
            .find_map(|e| {
                let base = e.resolved.as_ref().filter(|base| base.elite.is_none())?;
                (0..1024)
                    .filter_map(|key| catalog.elite_profile(base, key))
                    .find(|profile| {
                        profile
                            .elite
                            .as_ref()
                            .is_some_and(|mark| mark.affix_id == affix)
                    })
                    .map(|profile| (e.id, profile))
            })?;
        cast.push(pick);
    }
    let plain = a.enemies.iter().any(|e| {
        !e.boss
            && e.x < a.width_yards - 1.0
            && cast.iter().all(|(id, _)| *id != e.id)
            && e.resolved.as_ref().is_none_or(|p| p.elite.is_none())
    });
    plain.then_some(cast)
}

/// The cast hostiles as elites: each takes the catalogue's own elite profile
/// for its affix (presentation staging; hit points stay as spawned).
fn entitle_elites(f: &mut FrontendParts, cast: Vec<(u64, ResolvedEnemy)>) {
    let a = f.game.staging().attempt().expect("a live defense");
    for (id, profile) in cast {
        let enemy = a
            .enemies
            .iter_mut()
            .find(|e| e.id == id)
            .expect("a cast hostile");
        enemy.resolved = Some(profile);
    }
}

/// One live elite of every shipped affix beside plain hostiles: a real
/// stage-0 defense at `ELITE_TELLS_RANK`, played with the smoke input until
/// the field can cast every affix, then paused.
fn prepare_elite_tells(f: &mut FrontendParts) {
    entitle_start(f, "DEU");
    entitle_victories(f, ELITE_TELLS_RANK);
    deploy(f, 0, Mode::Personal);
    let mut cast = None;
    for _ in 0..ELITE_TELLS_TICK_CAP {
        cast = elite_tell_casting(f.game.attempt().expect("the defense stands"));
        if cast.is_some() {
            break;
        }
        let input = horde_core::sim::smoke_input(&f.game);
        f.game.tick(input);
    }
    let cast = cast.unwrap_or_else(|| {
        panic!("no hostile for every affix within {ELITE_TELLS_TICK_CAP} ticks")
    });
    entitle_elites(f, cast);
    pause(f);
}

/// The fx-stress effect load on top of the stress fixture: blood decals on,
/// a fourth burning zone with its Molotov burst, a grenade burst, and
/// `FX_STRESS_ELITE_PCT` percent elites spread over the crowd. Review-only
/// staging on a game that never saves; the `stress` perf scene stays bare.
#[cfg(feature = "review")]
pub(in crate::ui::review) fn entitle_fx_stress_load(f: &mut FrontendParts) {
    use horde_core::ordnance::{self, OrdnanceKind};
    use horde_core::sim::{Event, EventKind};
    f.settings.blood = true;
    let catalog = enemy_catalog();
    let a = f.game.staging().attempt().expect("the stress field");
    let centre = a.lane_count / 2;
    let mut zone = a.ordnance.fire_zones[0].clone();
    zone.id = a.next_id;
    zone.cell = [zone.cell[0] + 9, centre + 2];
    a.next_id += 2;
    let grenade = ordnance::catalog().definition(OrdnanceKind::Grenade);
    // A burst names its projectile: burning ground with that id is a
    // Molotov's, anything else a grenade's (`battle_view` rule).
    let bursts = [
        (zone.id, ordnance::cell_center(zone.cell), zone.radius),
        (
            zone.id + 1,
            ordnance::cell_center([zone.cell[0] - 4, centre.saturating_sub(2)]),
            grenade.radius,
        ),
    ];
    a.ordnance.fire_zones.push(zone);
    for (projectile, to, radius) in bursts {
        let seq = a.events.iter().map(|e| e.seq).max().unwrap_or(0) + 1;
        a.events.push_back(Event {
            kind: EventKind::Explosion,
            from: a.defender_position(),
            to,
            value: (radius * 100.0) as i32,
            tick: a.tick,
            actor_id: Some(projectile),
            target_id: None,
            lane: None,
            duration_ticks: 0,
            seq,
        });
    }
    let hostiles = a.enemies.iter().filter(|e| !e.boss).count();
    let wanted = (hostiles * FX_STRESS_ELITE_PCT).div_ceil(100);
    let is_elite = |e: &horde_core::sim::Enemy| {
        e.resolved
            .as_ref()
            .is_some_and(|profile| profile.elite.is_some())
    };
    let mut elites = a.enemies.iter().filter(|e| !e.boss && is_elite(e)).count();
    // Every third hostile first, so the elites spread over the whole crowd.
    for pass in 0..3 {
        for e in a.enemies.iter_mut().skip(pass).step_by(3) {
            if elites >= wanted {
                return;
            }
            if e.boss || e.split_child || is_elite(e) {
                continue;
            }
            let Some(profile) = e
                .resolved
                .as_ref()
                .and_then(|base| catalog.elite_profile(base, e.id))
            else {
                continue;
            };
            e.resolved = Some(profile);
            elites += 1;
        }
    }
}
