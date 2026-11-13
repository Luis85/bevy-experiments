//! Egui harness tests: briefing and preparation.
use super::*;

#[test]
fn preparation_keeps_the_planned_replay_and_defender_without_creating_an_attempt() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.campaign_mut().choose_start_country("DEU").unwrap();
    f.game.campaign_mut().attempts = 1;
    f.game
        .campaign_mut()
        .world
        .country_levels
        .insert("DEU".into(), 2);
    // The planned defender is core's next defender (W3-16, ruling D16).
    f.game
        .apply(&Command::SetNextDefender(
            horde_core::defenders::DefenderId::Mara,
        ))
        .unwrap();
    f.campaign.stage = 0;
    f.act(|c| c.open_preparation(Tab::Armory));
    let before = serde_json::to_value(&f.game).unwrap();
    f.act(|c| c.open_preparation(Tab::Briefing));
    h.frame(f, View::Camp, vec![]);
    assert!(f.nav.tab == Tab::Briefing);
    assert_eq!(f.campaign.stage, 0);
    assert_eq!(
        f.game.next_defender(),
        horde_core::defenders::DefenderId::Mara
    );
    // The camp's equipment and training pages follow the planned defender.
    assert!(f.campaign.mode == Mode::Npc);
    assert!(f.game.attempt().is_none());
    assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
}

#[test]
fn briefing_scene_line_humanises_weather_and_lighting_ids() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.campaign_mut().choose_start_country("DEU").unwrap();
    f.campaign.stage = 0;
    f.act(|c| c.open_preparation(Tab::Briefing));
    let output = h.frame_size(f, View::Camp, vec![], vec2(1440.0, 900.0));
    let country = world().countries.iter().find(|c| c.id == "DEU").unwrap();
    let scene = country.stage_theme(f.campaign.stage);
    let label = |id: &str| crate::ui::chrome::display_label(id);
    let line = format!(
        "{} · {} · {}",
        scene.title,
        label(&scene.weather),
        label(&scene.lighting)
    );
    assert!(
        output
            .shapes
            .iter()
            .any(|s| shape_has_text(&s.shape, &line)),
        "missing humanised scene line {line:?}"
    );
    let raw = format!("{} · {}", scene.weather, scene.lighting);
    assert!(
        !output.shapes.iter().any(|s| shape_has_text(&s.shape, &raw)),
        "raw theme ids leaked: {raw:?}"
    );
}

#[test]
fn briefing_loadout_and_readiness_sit_above_the_footer_at_both_review_sizes() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.campaign_mut().choose_start_country("DEU").unwrap();
    f.act(|c| c.open_preparation(Tab::Briefing));
    let ranged = f
        .game
        .campaign()
        .equipped(Mode::Personal, Slot::Ranged)
        .name
        .clone();
    for (size, scale) in [(vec2(1440.0, 900.0), 1.0), (vec2(1050.0, 700.0), 1.35)] {
        settings::configure(&h.ctx, scale);
        f.settings.scale = scale;
        let output = h.frame_size(f, View::WorldWithChrome, vec![], size);
        for label in ["Field loadout", "Change equipment", "Deploy personally"] {
            assert!(
                fully_visible(&output, label),
                "{size:?}: {label} hidden: {:?}",
                clipped_bounds(&output, label)
            );
        }
        assert!(
            output
                .shapes
                .iter()
                .any(|s| shape_has_text(&s.shape, &ranged)),
            "{size:?}: equipped ranged weapon not listed"
        );
        if scale == 1.0 {
            assert!(fully_visible(&output, "Before you deploy"), "{size:?}");
        }
    }
    h.click(f, View::Camp, "Change equipment");
    assert!(f.nav.tab == Tab::Armory);
}
