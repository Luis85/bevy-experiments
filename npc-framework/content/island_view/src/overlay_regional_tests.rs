use super::*;
use bevy::ecs::system::RunSystemOnce;
use npc_framework::kernel::hash::state_hash;
use npc_framework::view_helpers::nav_gizmos::NavGizmos;

#[test]
fn colony_profiles_show_navigation_only_when_diagnostics_are_requested() {
    for scene in ["living_world", "living_world"] {
        let mut app = island_content::new_sim_app(7, scene);
        app.insert_resource(OverlayState { visible: false })
            .insert_resource(crate::IslandMode::Editor)
            .init_resource::<NavGizmos>();
        let before = state_hash(app.world_mut());
        app.world_mut().run_system_once(colony_diagnostics).unwrap();
        assert!(
            !app.world().resource::<NavGizmos>().enabled,
            "{scene}: editor brush must not expose navigation diagnostics"
        );
        app.world_mut().resource_mut::<OverlayState>().visible = true;
        app.world_mut().run_system_once(colony_diagnostics).unwrap();
        assert!(
            app.world().resource::<NavGizmos>().enabled,
            "{scene}: explicit diagnostics remain available"
        );
        app.world_mut().insert_resource(crate::FrontendGate(true));
        app.world_mut().run_system_once(colony_diagnostics).unwrap();
        assert!(
            !app.world().resource::<NavGizmos>().enabled,
            "{scene}: menus suppress diagnostics"
        );
        assert_eq!(state_hash(app.world_mut()), before);
    }
}
