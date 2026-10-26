use crate::{
    DemoOptions,
    camera::{CameraRig, ExplorerCamera},
    interface::{Action, InputMode, UiQueue, UiState},
};
use bevy::{
    app::AppExit,
    prelude::*,
    render::view::screenshot::{Screenshot, save_to_disk},
};
use terra_bevy::rules::{Command, INTERACTION_REACH_M, ResourceNode, SandboxState};
use terra_bevy::{
    GeoActor, NetworkStatus, PoliticalWorld, ResidentChunks, SandboxIntent, SandboxQueue,
    SandboxRuntime, WorldEnvironment, WorldRuntime, WorldSet,
    render::{LayerVisual, actor_mesh},
};
use terra_core::{ActorKind, ActorState, GeoPosition, Layer, LocalFrame};

pub struct InteractionPlugin;
impl Plugin for InteractionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Placement>().add_systems(
            Update,
            (
                seed_world,
                shortcuts,
                actions,
                atlas_interaction,
                place_on_ground,
                verification,
            )
                .chain()
                .after(WorldSet::Stream)
                .before(WorldSet::Sandbox),
        );
    }
}
#[derive(Resource, Default)]
struct Placement(Option<PlaceKind>);
enum PlaceKind {
    Building(String),
    Crop,
}

fn ground_at(resident: &ResidentChunks, mut p: GeoPosition) -> Option<GeoPosition> {
    p.altitude_m = resident.terrain_height(p)?;
    Some(p)
}
fn offset(p: GeoPosition, east: f64, south: f64, resident: &ResidentChunks) -> Option<GeoPosition> {
    ground_at(resident, LocalFrame::new(p).to_geo([east, 0., south]).ok()?)
}

fn nearest_gatherable_node<'a>(
    state: &'a SandboxState,
    player_id: u64,
    position: GeoPosition,
    item: &str,
) -> Option<&'a ResourceNode> {
    let distance = |node: &ResourceNode| {
        position
            .surface_distance_m(node.position)
            .hypot(position.altitude_m - node.position.altitude_m)
    };
    state
        .resource_nodes
        .values()
        .filter(|node| {
            node.item == item
                && node.remaining > 0
                && node.position.validate().is_ok()
                && distance(node) <= INTERACTION_REACH_M
                && !state.plots.values().any(|plot| {
                    !state.may_interact(player_id, plot.faction_owner, plot.owner)
                        && plot.center.surface_distance_m(node.position) <= plot.radius_m
                })
        })
        .min_by(|a, b| distance(a).total_cmp(&distance(b)))
}

fn seed_world(
    mut done: Local<bool>,
    options: Res<DemoOptions>,
    resident: Res<ResidentChunks>,
    mut sandbox: ResMut<SandboxRuntime>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if *done || options.connect.is_some() {
        return;
    }
    let player = sandbox.player_id;
    let Some(position) = sandbox
        .state
        .players
        .get(&player)
        .map(|p| p.position)
        .and_then(|p| ground_at(&resident, p))
    else {
        return;
    };
    let _ = sandbox.state.set_player_position(player, position);
    for (id, east, item) in [
        ("authored:starter:wood", 6., "wood"),
        ("authored:starter:stone", -6., "stone"),
    ] {
        if let Some(p) = offset(position, east, 4., &resident) {
            let _ = sandbox.state.seed_resource(
                id.into(),
                p,
                item,
                100,
                if item == "wood" { 45. } else { 0. },
            );
        }
    }
    let material = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 0.8,
        cull_mode: None,
        ..default()
    });
    for (i, kind) in ActorKind::ALL.iter().copied().enumerate() {
        let Some(mut p) = offset(position, (i as f64 - 4.) * 25., -50., &resident) else {
            continue;
        };
        if kind == ActorKind::Ship {
            let water = resident
                .chunks
                .values()
                .flat_map(|c| &c.features)
                .filter(|f| f.kind == terra_core::FeatureKind::Water)
                .filter_map(|f| {
                    let frame = LocalFrame::new(position);
                    let mut sum = [0.; 3];
                    for vertex in &f.points {
                        let q = frame.to_local(*vertex);
                        for axis in 0..3 {
                            sum[axis] += q[axis];
                        }
                    }
                    let point = frame.to_geo(sum.map(|v| v / f.points.len() as f64)).ok()?;
                    let point = ground_at(&resident, point)?;
                    resident
                        .sample_environment(point, &terra_core::WeatherSample::default())
                        .filter(|e| e.water)
                        .map(|_| point)
                })
                .min_by(|a, b| {
                    a.surface_distance_m(position)
                        .total_cmp(&b.surface_distance_m(position))
                });
            let Some(water) = water else {
                continue;
            };
            p = water;
        }
        if matches!(
            kind,
            ActorKind::Helicopter | ActorKind::Airplane | ActorKind::Balloon
        ) {
            p.altitude_m += 70. + i as f64 * 8.;
        }
        let velocity = match kind {
            ActorKind::Airplane => [16., 8., 0.],
            ActorKind::Vehicle => [2., 0., 0.],
            ActorKind::Human | ActorKind::Animal | ActorKind::Creature => [0.7, 0.3, 0.],
            _ => [0.; 3],
        };
        commands.spawn((
            GeoActor(ActorState {
                id: 100 + i as u64,
                kind,
                position: p,
                velocity_enu_m_s: velocity,
                heading_deg: 0.,
            }),
            LayerVisual(Layer::Actors),
            Mesh3d(meshes.add(actor_mesh(kind))),
            MeshMaterial3d(material.clone()),
            Transform::default(),
        ));
    }
    sandbox.revision += 1;
    *done = true;
}

fn shortcuts(keys: Res<ButtonInput<KeyCode>>, ui: Res<UiState>, mut queue: ResMut<UiQueue>) {
    if ui.editing_gps {
        return;
    }
    for (key, action) in [
        (KeyCode::Space, Action::Pause),
        (KeyCode::F5, Action::Save),
        (KeyCode::F9, Action::Load),
        (KeyCode::Tab, Action::Follow),
        (KeyCode::KeyO, Action::Overview),
        (KeyCode::KeyM, Action::Atlas),
    ] {
        if keys.just_pressed(key) {
            queue.0.push_back(action);
        }
    }
}

fn actions(
    mut queue: ResMut<UiQueue>,
    mut ui: ResMut<UiState>,
    mut rig: ResMut<CameraRig>,
    mut runtime: ResMut<WorldRuntime>,
    resident: Res<ResidentChunks>,
    mut sandbox: ResMut<SandboxRuntime>,
    mut commands: ResMut<SandboxQueue>,
    mut placement: ResMut<Placement>,
    mut politics: ResMut<PoliticalWorld>,
    mut net: Option<ResMut<NetworkStatus>>,
) {
    for _ in 0..16 {
        let Some(action) = queue.0.pop_front() else {
            break;
        };
        let player_id = sandbox.player_id;
        let selected_faction = ui
            .selected_faction
            .filter(|id| sandbox.state.is_faction_member(player_id, *id))
            .or_else(|| {
                sandbox
                    .state
                    .factions
                    .values()
                    .find(|f| f.members.contains_key(&player_id))
                    .map(|f| f.id)
            });
        if !matches!(action, Action::NextFaction) {
            ui.selected_faction = selected_faction;
        }
        let position = sandbox
            .state
            .players
            .get(&player_id)
            .map(|p| p.position)
            .unwrap_or(runtime.focus);
        let command=match action{
            Action::Preset(index)=>{politics.atlas=false;placement.0=None;let presets=[(48.137,11.575,"Munich / Germany"),(47.421,10.985,"Zugspitze / Germany"),(53.541,9.985,"Hamburg / Germany"),(-33.9249,18.4241,"Cape Town / South Africa")];let(lat,lon,label)=presets[index];rig.teleport=GeoPosition::new(lat,lon,0.).ok();rig.altitude_offset=650.;ui.location=label.into();if let Some(net)=net.as_mut(){net.follow_player=false;}None},
            Action::EditGps=>{ui.input_mode=InputMode::Coordinates;ui.editing_gps=true;ui.gps_text.clear();None},
            Action::JumpGps=>{
                let value=ui.gps_text.trim().to_string();ui.editing_gps=false;
                match ui.input_mode {
                    InputMode::Coordinates=>{
                        let parts:Vec<_>=value.split(',').map(str::trim).collect();
                        let parsed=if parts.len()==2{parts[0].parse::<f64>().ok().zip(parts[1].parse::<f64>().ok()).and_then(|(lat,lon)|GeoPosition::new(lat,lon,0.).ok())}else{None};
                        if let Some(p)=parsed{politics.atlas=false;placement.0=None;rig.teleport=Some(p);rig.altitude_offset=650.;ui.location="Custom geographic location".into();if let Some(net)=net.as_mut(){net.follow_player=false;}}
                        else{sandbox.last_message="Enter valid latitude, longitude, e.g. 48.137, 11.575".into();}None
                    },
                    InputMode::FactionName=>Some(Command::CreateFaction{name:value}),
                    mode=>if let (Some(faction_id),Ok(target))=(selected_faction,value.parse::<u64>()){
                        Some(match mode {InputMode::InvitePlayer=>Command::InviteToFaction{faction_id,player_id:target},_=>Command::SetFactionRole{faction_id,player_id:target,role:if matches!(mode,InputMode::PromotePlayer){terra_bevy::rules::FactionRole::Builder}else{terra_bevy::rules::FactionRole::Member}}})
                    }else{sandbox.last_message="Select a faction and enter a valid player ID.".into();None},
                }
            },
            Action::Toggle(layer)=>{let enabled=runtime.config.layers.contains(layer);runtime.config.layers.set(layer,!enabled);None},
            Action::Pause=>{if !runtime.network_authority{runtime.clock.paused=!runtime.clock.paused;}None},
            Action::Speed=>{if !runtime.network_authority{runtime.clock.time_scale=if runtime.clock.time_scale<10.{60.}else if runtime.clock.time_scale<100.{600.}else{1.};}None},
            Action::Noon=>{if !runtime.network_authority{runtime.clock.unix_seconds=(runtime.clock.unix_seconds/86400.).floor()*86400.+((12.-runtime.focus.longitude/15.).rem_euclid(24.))*3600.;}else{sandbox.last_message="The server controls the calendar in multiplayer.".into();}None},
            Action::Season=>{if !runtime.network_authority{runtime.clock.unix_seconds+=91.*86400.;}None},
            Action::Camp=>{if let Some(p)=ground_at(&resident,runtime.focus){commands.0.push_back(SandboxIntent::MoveTo(p));}else{sandbox.last_message="Wait for terrain before moving the player.".into();}None},
            Action::Claim=>Some(Command::ClaimPlot{center:position,radius_m:40.}),
            Action::GatherWood|Action::GatherStone=>{let item=if matches!(action,Action::GatherWood){"wood"}else{"stone"};if let Some(node)=nearest_gatherable_node(&sandbox.state,player_id,position,item){Some(Command::Gather{node_id:node.id.clone(),amount:5.min(node.remaining)})}else{sandbox.last_message=format!("No permitted {item} resource within {INTERACTION_REACH_M:.0} m.");None}},
            Action::Planks=>Some(Command::Craft{recipe:"planks".into(),count:1}),Action::Blocks=>Some(Command::Craft{recipe:"stone_blocks".into(),count:1}),
            Action::Tool=>Some(Command::Craft{recipe:"tool".into(),count:1}),
            Action::Seeds=>Some(Command::Craft{recipe:"wheat_seed".into(),count:1}),
            Action::House|Action::Workbench|Action::Fence=>{politics.atlas=false;let blueprint=match action{Action::House=>"house",Action::Workbench=>"workbench",_=>"fence"};placement.0=Some(PlaceKind::Building(blueprint.into()));sandbox.last_message=format!("Place {blueprint}: click ground in your plot within 20 m of your player. Esc cancels.");None},
            Action::Plant=>{politics.atlas=false;placement.0=Some(PlaceKind::Crop);sandbox.last_message="Plant wheat: click free ground inside your plot, within 20 m. Esc cancels.".into();None},
            Action::Harvest=>sandbox.state.farms.values().filter(|f|sandbox.state.may_interact(player_id,f.faction_owner,f.owner)&&f.ready()).min_by(|a,b|a.position.surface_distance_m(position).total_cmp(&b.position.surface_distance_m(position))).map(|f|Command::Harvest{farm_id:f.id}).or_else(||{sandbox.last_message="No owned crop is ripe yet. Growth depends on time, temperature and moisture.".into();None}),
            Action::Damage=>sandbox.state.structures.values().filter(|s|sandbox.state.may_interact(player_id,s.faction_owner,s.owner)).min_by(|a,b|a.position.surface_distance_m(position).total_cmp(&b.position.surface_distance_m(position))).map(|s|Command::Damage{structure_id:s.id,amount:25}).or_else(||{sandbox.last_message="No structure with your building permission is nearby.".into();None}),
            Action::Home=>sandbox.state.structures.values().filter(|s|sandbox.state.may_use(player_id,s.faction_owner,s.owner)&&s.housing).min_by(|a,b|a.position.surface_distance_m(position).total_cmp(&b.position.surface_distance_m(position))).map(|s|Command::SetHome{structure_id:s.id}).or_else(||{sandbox.last_message="Build a house or join a faction with a shared house first.".into();None}),
            Action::CreateFaction|Action::InviteFaction|Action::PromoteFaction|Action::DemoteFaction=>{
                ui.input_mode=match action{Action::CreateFaction=>InputMode::FactionName,Action::InviteFaction=>InputMode::InvitePlayer,Action::PromoteFaction=>InputMode::PromotePlayer,_=>InputMode::DemotePlayer};ui.gps_text.clear();ui.editing_gps=true;None
            },
            Action::AcceptFaction=>sandbox.state.factions.values().find(|f|f.invites.contains(&player_id)).map(|f|Command::AcceptFactionInvite{faction_id:f.id}).or_else(||{sandbox.last_message="No pending faction invitation.".into();None}),
            Action::NextFaction=>{let ids:Vec<_>=sandbox.state.factions.values().filter(|f|f.members.contains_key(&player_id)).map(|f|f.id).collect();if !ids.is_empty(){let next=ids.iter().position(|id|Some(*id)==selected_faction).map(|i|(i+1)%ids.len()).unwrap_or(0);ui.selected_faction=Some(ids[next]);}None},
            Action::SharePlot=>{
                let plot=sandbox.state.plots.values().filter(|p|p.owner==player_id&&p.faction_owner.is_none()).min_by(|a,b|a.center.surface_distance_m(position).total_cmp(&b.center.surface_distance_m(position)));
                if let(Some(plot),Some(faction_id))=(plot,selected_faction){Some(Command::TransferPlotToFaction{plot_id:plot.id,faction_id})}else{sandbox.last_message="Select your faction and claim a personal plot first.".into();None}
            },
            Action::Atlas=>{politics.atlas=!politics.atlas;placement.0=None;None},
            Action::FitAtlas=>{sandbox.last_message=if politics.fit_at_epoch(runtime.clock.unix_seconds as i64){"Atlas framed to geometry valid at the simulation date.".into()}else{"No political geometry is valid at this date.".into()};None},
            Action::Borders=>{politics.borders=!politics.borders;None},
            Action::FlyToSelection=>{if let Some(p)=politics.selected{rig.teleport=Some(p);rig.altitude_offset=650.;politics.atlas=false;ui.location="Selected atlas location".into();if let Some(net)=net.as_mut(){net.follow_player=false;}}else{sandbox.last_message="Click a location in the political atlas first.".into();}None},
            Action::DrawTerritory=>{if runtime.network_authority{sandbox.last_message="Draw scenario territories in a local session, export, then configure every peer with that dataset.".into();}else if politics.dataset.is_none(){sandbox.last_message="Load a political GeoJSON dataset before drawing territories.".into();}else{politics.atlas=true;politics.drawing=true;politics.draft.clear();placement.0=None;sandbox.last_message="Click territory vertices. Enter closes the boundary; Esc cancels. This is an authored scenario layer.".into();}None},
            Action::ExportPolitics=>{let path=runtime.config.save_path.with_extension("politics.geojson");sandbox.last_message=politics.export(&path).map(|_|format!("Exported {}",path.display())).unwrap_or_else(|e|format!("Export failed: {e}"));None},
            Action::Save=>{commands.0.push_back(SandboxIntent::Save);None},Action::Load=>{commands.0.push_back(SandboxIntent::Load);None},
            Action::Follow=>{politics.atlas=false;placement.0=None;if let Some(net)=net.as_mut(){net.follow_player=true;}else{rig.position=LocalFrame::new(position).to_geo([15.,22.,32.]).unwrap_or(position);rig.yaw=0.42;rig.pitch=-0.52;rig.needs_grounding=false;}None},
            Action::Overview=>{politics.atlas=false;rig.position=LocalFrame::new(position).to_geo([200.,650.,650.]).unwrap_or(position);rig.yaw=-0.28;rig.pitch=-0.65;rig.needs_grounding=false;if let Some(net)=net.as_mut(){net.follow_player=false;}None},
        };
        if let Some(command) = command {
            commands.0.push_back(SandboxIntent::Command(command));
        }
    }
}

fn pick_ground(
    camera: &Camera,
    transform: &GlobalTransform,
    cursor: Vec2,
    runtime: &WorldRuntime,
    resident: &ResidentChunks,
) -> Option<GeoPosition> {
    let ray = camera.viewport_to_world(transform, cursor).ok()?;
    let at = |distance: f32| -> Option<(GeoPosition, f64)> {
        let p = ray.origin + *ray.direction * distance;
        let geo = runtime.frame.to_geo(p.to_array().map(|x| x as f64)).ok()?;
        Some((geo, geo.altitude_m - resident.terrain_height(geo)?))
    };
    let mut previous = 0.;
    for i in 1..=500 {
        let distance = i as f32 * 20.;
        if let Some((_, height)) = at(distance) {
            if height <= 0. {
                let mut low = previous;
                let mut high = distance;
                for _ in 0..14 {
                    let mid = (low + high) * 0.5;
                    if at(mid).is_some_and(|(_, h)| h > 0.) {
                        low = mid;
                    } else {
                        high = mid;
                    }
                }
                return at((low + high) * 0.5).and_then(|(p, _)| ground_at(resident, p));
            }
        }
        previous = distance;
    }
    None
}

fn place_on_ground(
    mut placement: ResMut<Placement>,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    window: Single<&Window>,
    camera: Single<(&Camera, &GlobalTransform), With<ExplorerCamera>>,
    runtime: Res<WorldRuntime>,
    politics: Res<PoliticalWorld>,
    resident: Res<ResidentChunks>,
    sandbox: Res<SandboxRuntime>,
    mut queue: ResMut<SandboxQueue>,
    mut gizmos: Gizmos,
) {
    if politics.atlas {
        return;
    }
    if keys.just_pressed(KeyCode::Escape) {
        placement.0 = None;
    }
    let Some(kind) = &placement.0 else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    if cursor.x < 280.
        || cursor.x > window.width() - 296.
        || cursor.y < 134.
        || cursor.y > window.height() - 96.
    {
        return;
    }
    let Some(position) = pick_ground(camera.0, camera.1, cursor, &runtime, &resident) else {
        return;
    };
    let Some(player) = sandbox.state.players.get(&sandbox.player_id) else {
        return;
    };
    let plot = sandbox.state.plots.values().find(|p| {
        sandbox.state.may_build(player.id, p.faction_owner, p.owner)
            && p.center.surface_distance_m(position) < p.radius_m - 4.
    });
    let valid = plot.is_some()
        && player
            .position
            .surface_distance_m(position)
            .hypot(player.position.altitude_m - position.altitude_m)
            <= 20.;
    let center = Vec3::from_array(runtime.frame.to_local(position).map(|v| v as f32));
    let size = match kind {
        PlaceKind::Building(b) if b == "house" => Vec3::new(7., 5., 6.),
        PlaceKind::Building(_) => Vec3::new(2., 1.5, 1.5),
        _ => Vec3::new(2., 0.2, 2.),
    };
    gizmos.cube(
        Transform::from_translation(center + Vec3::Y * size.y * 0.5).with_scale(size),
        if valid {
            Color::srgb(0.5, 1., 0.5)
        } else {
            Color::srgb(1., 0.3, 0.2)
        },
    );
    if buttons.just_pressed(MouseButton::Left) && valid {
        let command = match kind {
            PlaceKind::Building(blueprint) => Some(Command::Build {
                blueprint: blueprint.clone(),
                position,
            }),
            PlaceKind::Crop => plot.map(|plot| Command::Plant {
                plot_id: plot.id,
                position,
                crop: "wheat".into(),
            }),
        };
        if let Some(command) = command {
            queue.0.push_back(SandboxIntent::Command(command));
            placement.0 = None;
        }
    }
}

fn verification(
    mut frames: Local<u64>,
    mut exercised: Local<bool>,
    mut captured: Local<bool>,
    mut failed: Local<bool>,
    render: Res<terra_bevy::render::RenderTelemetry>,
    options: Res<DemoOptions>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
    resident: Res<ResidentChunks>,
    mut sandbox: ResMut<SandboxRuntime>,
    runtime: Res<WorldRuntime>,
) {
    *frames += 1;
    if options.smoke && !*exercised && !resident.chunks.is_empty() {
        let player = sandbox.player_id;
        let p = sandbox.state.players[&player].position;
        let plan = [
            Command::ClaimPlot {
                center: p,
                radius_m: 40.,
            },
            Command::Craft {
                recipe: "planks".into(),
                count: 5,
            },
            Command::Craft {
                recipe: "stone_blocks".into(),
                count: 4,
            },
        ];
        let mut failures = Vec::new();
        for command in plan {
            let SandboxRuntime { state, catalog, .. } = &mut *sandbox;
            if let Err(e) = state.apply(player, command, catalog) {
                failures.push(e.to_string());
            }
        }
        let plot = sandbox
            .state
            .plots
            .values()
            .find(|p| p.owner == player)
            .map(|p| p.id);
        for (blueprint, x, z) in [
            ("house", 0., -8.),
            ("workbench", 8., -3.),
            ("fence", -8., -4.),
        ] {
            if let Some(position) = offset(p, x, z, &resident) {
                let SandboxRuntime { state, catalog, .. } = &mut *sandbox;
                if let Err(e) = state.apply(
                    player,
                    Command::Build {
                        blueprint: blueprint.into(),
                        position,
                    },
                    catalog,
                ) {
                    failures.push(e.to_string());
                }
            }
        }
        if let (Some(plot_id), Some(position)) = (plot, offset(p, -8., 7., &resident)) {
            let SandboxRuntime { state, catalog, .. } = &mut *sandbox;
            if let Err(e) = state.apply(
                player,
                Command::Plant {
                    plot_id,
                    position,
                    crop: "wheat".into(),
                },
                catalog,
            ) {
                failures.push(e.to_string());
            }
        }
        if let Some(id) = sandbox
            .state
            .structures
            .values()
            .find(|s| s.housing)
            .map(|s| s.id)
        {
            let SandboxRuntime { state, catalog, .. } = &mut *sandbox;
            let _ = state.apply(player, Command::SetHome { structure_id: id }, catalog);
        }
        sandbox.revision += 1;
        *exercised = true;
        *failed = !failures.is_empty();
        sandbox.last_message = if failures.is_empty() {
            "Verified: land claimed, materials crafted, house/workbench/fence built, home set, wheat planted.".into()
        } else {
            format!("Smoke failures: {failures:?}")
        };
        println!(
            "TERRA_SMOKE {}",
            serde_json::json!({"success":failures.is_empty(),"structures":sandbox.state.structures.len(),"farms":sandbox.state.farms.len(),"chunks":resident.chunks.len(),"errors":failures})
        );
    }
    let expected = terra_core::TileKey::at(runtime.focus, runtime.config.chunk_level)
        .map(|key| key.neighbors(runtime.config.chunk_radius).len())
        .unwrap_or(1);
    if !*captured
        && *frames >= 80
        && resident.chunks.len() >= expected
        && runtime.stream_stats.in_flight == 0
        && render.pending_meshes == 0
        && render.building_meshes == 0
    {
        if let Some(path) = &options.screenshot {
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(path.clone()));
            *captured = true;
        }
    }
    if options.exit_frames.is_some_and(|f| *frames >= f) {
        println!(
            "TERRA_RUN {}",
            serde_json::json!({"frames":*frames,"chunks":resident.chunks.len(),"rebases":runtime.rebases,"stream_poll_ms":runtime.stream_poll_ms,"sim_ms":runtime.simulation_ms})
        );
        exit.write(
            if *failed
                || (options.smoke && !*exercised)
                || (options.screenshot.is_some() && !*captured)
            {
                AppExit::error()
            } else {
                AppExit::Success
            },
        );
    }
}

fn atlas_interaction(
    window: Single<&Window>,
    camera: Single<(&Camera, &GlobalTransform), With<ExplorerCamera>>,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    runtime: Res<WorldRuntime>,
    ui: Res<UiState>,
    mut politics: ResMut<PoliticalWorld>,
    mut sandbox: ResMut<SandboxRuntime>,
) {
    if !politics.atlas || ui.editing_gps {
        return;
    }
    if politics.drawing {
        if keys.just_pressed(KeyCode::Escape) {
            politics.draft.clear();
            politics.drawing = false;
            sandbox.last_message = "Territory drawing cancelled.".into();
        }
        if keys.just_pressed(KeyCode::Backspace) {
            politics.draft.pop();
        }
        if keys.just_pressed(KeyCode::Enter) {
            let faction = ui
                .selected_faction
                .filter(|id| sandbox.state.is_faction_member(sandbox.player_id, *id));
            sandbox.last_message = politics
                .finish_draft(runtime.clock.unix_seconds as i64, faction)
                .unwrap_or_else(|e| format!("Territory rejected: {e}"));
        }
    }
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    if cursor.x < 280.
        || cursor.x > window.width() - 296.
        || cursor.y < 184.
        || cursor.y > window.height() - 220.
    {
        return;
    }
    let Ok(ray) = camera.0.viewport_to_world(camera.1, cursor) else {
        return;
    };
    if ray.direction.z.abs() < 1e-6 {
        return;
    }
    let point = ray.origin + *ray.direction * (-ray.origin.z / ray.direction.z);
    if point.x.abs() > 500. || point.y.abs() > 250. {
        return;
    }
    let Ok(position) = GeoPosition::new(
        (point.y / 250. * 90.) as f64,
        (point.x / 500. * 180.) as f64,
        0.,
    ) else {
        return;
    };
    politics.hover = format!(
        "{:.3}°, {:.3}° · {}",
        position.latitude,
        position.longitude,
        politics.describe(position, &runtime.clock)
    );
    if buttons.just_pressed(MouseButton::Left) {
        politics.selected = Some(position);
        if politics.drawing {
            if politics.draft.len() < 128 {
                politics.draft.push(position);
            } else {
                sandbox.last_message = "Territory vertex limit reached. Enter closes it.".into();
            }
        } else {
            sandbox.last_message = format!(
                "Selected {:.3}°, {:.3}°. Use ‘Fly to selected map location’ to explore here.",
                position.latitude, position.longitude
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use terra_bevy::rules::Catalog;

    #[test]
    fn gathering_skips_nearer_forbidden_and_vertically_unreachable_nodes() {
        let position = GeoPosition::new(48.137, 11.575, 100.).unwrap();
        let frame = LocalFrame::new(position);
        let forbidden = frame.to_geo([3., 0., 0.]).unwrap();
        let allowed = frame.to_geo([-10., 0., 0.]).unwrap();
        let above = frame.to_geo([-4., 30., 0.]).unwrap();
        let catalog = Catalog::builtin();
        let mut state = SandboxState::default();
        state.add_player(1, position).unwrap();
        state.add_player(2, forbidden).unwrap();
        state
            .apply(
                2,
                Command::ClaimPlot {
                    center: forbidden,
                    radius_m: 5.,
                },
                &catalog,
            )
            .unwrap();
        for (id, point) in [
            ("forbidden", forbidden),
            ("allowed", allowed),
            ("above", above),
        ] {
            state
                .seed_resource(id.into(), point, "wood", 10, 0.)
                .unwrap();
        }

        let target = nearest_gatherable_node(&state, 1, position, "wood").unwrap();
        assert_eq!(target.id, "allowed");
        let command = Command::Gather {
            node_id: target.id.clone(),
            amount: 5,
        };
        state.apply(1, command, &catalog).unwrap();
        assert_eq!(state.resource_nodes["allowed"].remaining, 5);
        assert_eq!(state.resource_nodes["forbidden"].remaining, 10);
        assert_eq!(state.resource_nodes["above"].remaining, 10);

        state.resource_nodes.get_mut("allowed").unwrap().remaining = 0;
        assert!(nearest_gatherable_node(&state, 1, position, "wood").is_none());
    }
}
