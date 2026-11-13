//! The survivor rig's model follows the appearance stamped on the attempt's
//! profile (W3-17, ruling D17): an Npc defense is not forced to the Female
//! model, and main-run Mara, whose deploy stamps Female, is drawn as before.
use super::*;
use horde_core::{content::Mode, defenders::DefenderId, survivors::Appearance};

/// A paused main-run companion defense (Mara's Npc deploy).
fn companion_defense() -> Game {
    let mut game = Game::default();
    // A compatibility campaign meets Mara after its introductory attempt.
    game.campaign_mut().legacy_features = true;
    game.campaign_mut().choose_start_country("DEU").unwrap();
    game.campaign_mut().attempts = 1;
    game.deploy_country_stage(0, Mode::Npc).unwrap();
    game.set_paused(true);
    game
}

fn view_of(a: &horde_core::sim::Attempt) -> BattleViewModel {
    BattleViewModel::build(a, SceneAnimation::sample(a, false), SceneBlend::NONE, false)
}

/// Every shape the battlefield of `game` paints on a fresh context.
fn field(game: &Game) -> Vec<egui::Shape> {
    let a = game.attempt().expect("a live defense");
    let view = view_of(a);
    painted(|p| {
        battlefield(p, arena(), &view, a, [10.0, 5.0], options(textures()));
    })
}

fn managed_meshes(shape: &egui::Shape, into: &mut Vec<egui::Mesh>) {
    match shape {
        egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| managed_meshes(shape, into)),
        egui::Shape::Mesh(mesh) => {
            if matches!(mesh.texture_id, TextureId::Managed(_)) {
                into.push((**mesh).clone());
            }
        }
        _ => {}
    }
}

/// The meshes the battlefield of `game` draws from egui-managed textures:
/// the survivor rig's parts, sampled from the player source sheets (every
/// other sheet is one of the fake user ids of [`textures`]). The two models
/// share source sheets, so the parts' regions tell them apart. A fresh
/// context uploads the sources in catalogue order, so the texture ids
/// compare across calls.
fn rig_meshes(game: &Game) -> Vec<egui::Mesh> {
    let mut drawn = Vec::new();
    for shape in field(game) {
        managed_meshes(&shape, &mut drawn);
    }
    drawn
}

/// Ruling D17: the rig is drawn from the appearance stamped on the profile.
/// An Npc defense is no longer forced to the Female model, so a recruit
/// stamped Male is drawn Male.
#[test]
fn battlefield_draws_profile_appearance_not_forced_female() {
    let mut game = companion_defense();
    a_mut(&mut game).profile.appearance = Appearance::Male;
    let male = rig_meshes(&game);
    a_mut(&mut game).profile.appearance = Appearance::Female;
    let female = rig_meshes(&game);
    assert!(
        !male.is_empty() && !female.is_empty(),
        "the rig draws from its source sheets"
    );
    // `assert!`, not `assert_ne!`: a failure must not dump the meshes.
    assert!(
        male != female,
        "an Npc defense stamped Male is drawn with the Male model, not a forced Female one"
    );
}

/// A grenade in flight over a paused Personal defense.
fn throw_in_flight() -> Game {
    use horde_core::{
        balance::BalanceSettings,
        ordnance::{OrdnanceKind, ThrowRequest},
        sim::Input,
    };
    let mut game = Game::default();
    game.campaign_mut()
        .set_balance_settings(BalanceSettings::legacy())
        .unwrap();
    game.campaign_mut().coins = 1000;
    game.campaign_mut()
        .purchase_node("grenade_training")
        .unwrap();
    game.deploy(0, Mode::Personal).unwrap();
    game.set_paused(false);
    game.tick(Input {
        throw: Some(ThrowRequest {
            kind: OrdnanceKind::Grenade,
            cell: [8, 5],
        }),
        ..Default::default()
    });
    for _ in 0..6 {
        game.tick(Input::default());
    }
    game.set_paused(true);
    assert!(!game.attempt().unwrap().ordnance.projectiles.is_empty());
    game
}

/// What `ordnance::thrown` paints for `game` on a fresh context.
fn thrown_shapes(game: &Game) -> Vec<egui::Shape> {
    let a = game.attempt().unwrap();
    let view = view_of(a);
    assert!(!view.projectiles.is_empty(), "a projectile in flight");
    painted(|p| {
        ordnance::thrown(
            p,
            ArenaProjection::for_attempt(arena(), a),
            &view,
            a,
            false,
            textures(),
            &a.profile.equipment_visual,
        );
    })
}

/// The projectile's release origin follows the same rule as the rig (D17):
/// an Npc defense stamped Male throws from the Male model's socket.
#[test]
fn thrown_release_origin_reads_profile_appearance_not_forced_female() {
    let mut game = throw_in_flight();
    let a = a_mut(&mut game);
    a.mode = Mode::Npc;
    a.profile.appearance = Appearance::Male;
    let male = thrown_shapes(&game);
    a_mut(&mut game).profile.appearance = Appearance::Female;
    let female = thrown_shapes(&game);
    assert!(
        male != female,
        "the throw leaves the stamped model's socket, not a forced Female one"
    );
}

/// Pin (W3-17): main-run Mara is drawn exactly as before the forced-Female
/// branch was deleted. Her deploy stamps Female on the profile, so reading
/// the profile picks the model the deleted branch forced: the whole field
/// paints the same shapes as an attempt whose appearance is set to Female by
/// hand. Perturbation proof: stamping Male for Npc deploys in
/// `Campaign::defender_profile` (campaign/loadout.rs) fails the appearance
/// assertion here.
#[test]
fn mara_battlefield_model_unchanged() {
    let mut game = companion_defense();
    let a = game.attempt().unwrap();
    assert_eq!(a.defender(), DefenderId::Mara);
    assert_eq!(a.defender_appearance(), Appearance::Female);
    let stamped = field(&game);
    assert!(
        !rig_meshes(&game).is_empty(),
        "the rig draws from its source sheets"
    );
    a_mut(&mut game).profile.appearance = Appearance::Female;
    assert!(field(&game) == stamped, "forcing Female changes nothing");
}
