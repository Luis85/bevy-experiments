//! Egui harness tests: briefing and preparation.
use super::*;

#[test]
fn preparation_keeps_the_planned_replay_and_defender_without_creating_an_attempt() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.campaign.choose_start_country("DEU").unwrap();
    f.game.campaign.attempts = 1;
    f.game.campaign.world.country_levels.insert("DEU".into(), 2);
    f.campaign.mode = Mode::Npc;
    f.campaign.stage = 0;
    f.act(|c| c.open_preparation(Tab::Armory));
    let before = serde_json::to_value(&f.game).unwrap();
    f.act(|c| c.open_preparation(Tab::Briefing));
    h.frame(f, View::Camp, vec![]);
    assert!(f.nav.tab == Tab::Briefing);
    assert_eq!(f.campaign.stage, 0);
    assert!(f.campaign.mode == Mode::Npc);
    assert!(f.game.attempt.is_none());
    assert_eq!(serde_json::to_value(&f.game).unwrap(), before);
}

#[test]
fn briefing_scene_line_humanises_weather_and_lighting_ids() {
    let mut fixture = IsolatedFrontend::new();
    let h = Harness::new();
    let f = &mut fixture.frontend;
    f.game.campaign.choose_start_country("DEU").unwrap();
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
