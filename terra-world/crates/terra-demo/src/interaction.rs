use crate::{
    DemoOptions,
    camera::{CameraRig, ExplorerCamera},
    control::{ControlIntent, ControlQueue, PlayerControl, WorldViewport},
    interface::{Action, InputMode, UiQueue, UiState},
};
use bevy::{
    app::AppExit,
    prelude::*,
    render::view::screenshot::{Screenshot, save_to_disk},
};
use terra_bevy::rules::{Command, INTERACTION_REACH_M, LivingCommand, ResourceNode, SandboxState};
use terra_bevy::{
    NetworkStatus, PoliticalWorld, ResidentChunks, SandboxIntent, SandboxQueue, SandboxRuntime,
    WorldEnvironment, WorldRuntime, WorldSet,
};
use terra_core::{GeoPosition, LocalFrame};

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
pub(crate) struct Placement(pub Option<PlaceKind>);
impl Placement {
    pub(crate) fn is_active(&self) -> bool {
        self.0.is_some()
    }
}
pub(crate) enum PlaceKind {
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
                && state
                    .check_settlement_interact(player_id, node.position)
                    .is_ok()
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
    runtime: Res<WorldRuntime>,
    persistence: Res<terra_bevy::RegionalPersistence>,
    mut queue: ResMut<SandboxQueue>,
) {
    if *done || options.connect.is_some() || runtime.network_authority {
        return;
    }
    // A resumed world already owns its exact pose and starter-resource history.
    // Running this demo initializer again can ground a possessed aircraft or
    // reinsert an authored resource whose page is currently dormant.
    if persistence.restored {
        *done = true;
        return;
    }
    // Fresh initialization runs only in a complete, unblocked working set.
    // Stream integration hydrates before this system when terrain arrives; if
    // the overlay is partial, wait for that barrier instead of guessing that a
    // missing starter ID has never existed. No private store cursor is bypassed.
    if persistence.blocked() || persistence.pending() || sandbox.state.is_partial() {
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
    let result = (|| -> Result<SandboxState, String> {
        let mut state = sandbox.state.clone();
        // The living adapter has its own safe placement and clearance policy.
        // This fallback only grounds the initial unbodied reference player.
        if !state.living.enabled {
            state
                .set_player_position(player, position)
                .map_err(|e| e.to_string())?;
        }
        for (id, east, item) in [
            ("authored:starter:wood", 6., "wood"),
            ("authored:starter:stone", -6., "stone"),
        ] {
            if state.resource_nodes.contains_key(id) || !sandbox.catalog.items.contains(item) {
                continue;
            }
            if let Some(p) = offset(position, east, 4., &resident) {
                state
                    .seed_resource(
                        id.into(),
                        p,
                        item,
                        100,
                        if item == "wood" { 45. } else { 0. },
                    )
                    .map_err(|e| e.to_string())?;
            }
        }
        state
            .validate_with_catalog(&sandbox.catalog)
            .map_err(|e| e.to_string())?;
        Ok(state)
    })();
    let state = match result {
        Ok(state) => state,
        Err(error) => {
            sandbox.last_message = format!("Starter world initialization deferred: {error}");
            return;
        }
    };
    sandbox.state = state;
    sandbox.revision = sandbox.revision.wrapping_add(1);
    sandbox.dirty = true;
    if persistence.enabled {
        // The normal queue consumer commits and retires through RegionStore.
        // This also covers a fresh complete world with already-resident terrain
        // where no new StreamEvent would otherwise request a checkpoint.
        queue.0.push_back(SandboxIntent::Save);
    }
    *done = true;
}

fn shortcuts(
    keys: Res<ButtonInput<KeyCode>>,
    input: Res<crate::player_input::PlayerInput>,
    settings: Res<crate::player_input::InputSettings>,
    ui: Res<UiState>,
    mut queue: ResMut<UiQueue>,
    sandbox: Res<SandboxRuntime>,
    flow: Option<Res<crate::flow::GameFlow>>,
) {
    if input.world_blocked() || ui.editing_gps || flow.as_ref().is_some_and(|f| f.blocks_input()) {
        return;
    }
    for (key, action) in [
        (KeyCode::Space, Action::Pause),
        (KeyCode::F5, Action::Save),
        (KeyCode::F9, Action::Load),
        (KeyCode::Tab, Action::Follow),
        (KeyCode::KeyO, Action::Overview),
        (KeyCode::KeyY, Action::Panel(crate::interface::Panel::Fleet)),
        (KeyCode::KeyM, Action::Atlas),
        (KeyCode::F10, Action::ToggleWorld),
        (KeyCode::F11, Action::ToggleInspector),
        (KeyCode::F12, Action::Diagnostics),
        (
            KeyCode::KeyJ,
            Action::Panel(crate::interface::Panel::Activities),
        ),
        (
            KeyCode::KeyN,
            Action::Panel(crate::interface::Panel::Nature),
        ),
        (
            KeyCode::KeyT,
            Action::Panel(crate::interface::Panel::Territory),
        ),
        (
            KeyCode::KeyV,
            Action::Panel(crate::interface::Panel::Driving),
        ),
        (
            KeyCode::KeyG,
            Action::Panel(crate::interface::Panel::Garage),
        ),
    ] {
        let space_is_control = key == KeyCode::Space
            && (settings.space_has_rts_action()
                || sandbox
                    .state
                    .living
                    .accounts
                    .get(&sandbox.player_id)
                    .is_some_and(|a| a.possessed));
        if keys.just_pressed(key) && !space_is_control {
            queue.0.push_back(action);
        }
    }
}

fn body_transition_pending(commands: &SandboxQueue, controls: &ControlQueue) -> bool {
    controls.0.iter().any(|intent| {
        matches!(
            intent,
            ControlIntent::SetMode(_)
                | ControlIntent::PossessSelected
                | ControlIntent::Release
                | ControlIntent::SelectAll
                | ControlIntent::CycleActor
        )
    }) || commands.0.iter().any(|intent| {
        matches!(intent, SandboxIntent::Load | SandboxIntent::LoadReviewed(_))
            || matches!(
                intent,
                SandboxIntent::Command(Command::Living {
                    action: LivingCommand::SelectActive { .. }
                        | LivingCommand::Possess { .. }
                        | LivingCommand::Release,
                }) | SandboxIntent::Command(Command::Transport {
                    action: terra_bevy::rules::TransportCommand::EnterVehicle { .. }
                        | terra_bevy::rules::TransportCommand::ExitVehicle,
                })
            )
    })
}

pub(crate) fn actions(
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
    mut control_queue: ResMut<ControlQueue>,
    mut control: ResMut<PlayerControl>,
    presentation: (
        ResMut<terra_bevy::render::RenderQualitySettings>,
        Option<Res<terra_bevy::render::ActorSceneOverrides>>,
        Option<Res<DemoOptions>>,
    ),
    mut timings: ResMut<terra_bevy::WorldDiagnostics>,
    persistence: Res<terra_bevy::RegionalPersistence>,
    mut flow: Option<ResMut<crate::flow::GameFlow>>,
) {
    let (mut quality, scenes, options) = presentation;
    if flow.as_ref().is_some_and(|f| f.blocks_input()) {
        queue.0.clear();
        return;
    }
    for _ in 0..16 {
        let Some(action) = queue.0.pop_front() else {
            break;
        };
        if persistence.authority_paused()
            && matches!(
                action,
                Action::Pause
                    | Action::Speed
                    | Action::Season
                    | Action::Noon
                    | Action::DrawTerritory
            )
        {
            sandbox.last_message = "Persistence is blocked. F5 retries the checkpoint; F9 restores the last committed world.".into();
            continue;
        }
        let player_id = sandbox.player_id;
        let selected_faction =
            crate::communities::selected(&sandbox, &ui).map(|faction| faction.id);
        // An explicit missing selection stays missing until deliberate browsing.
        if ui.selected_faction.is_none() && !matches!(action, Action::NextFaction) {
            ui.selected_faction = selected_faction;
        }
        if matches!(
            action,
            Action::IndustryReviewed { .. }
                | Action::FleetReviewed { .. }
                | Action::ReviewDamage(_)
                | Action::ConfirmDamageReviewed(_)
        ) && (body_transition_pending(&commands, &control_queue)
            || net.as_ref().is_some_and(|net| !net.connected))
        {
            sandbox.last_message = "The active actor or world is still changing. Review the named operation after the transition.".into();
            continue;
        }
        let position = sandbox
            .state
            .players
            .get(&player_id)
            .map(|p| p.position)
            .unwrap_or(runtime.focus);
        let command=match action{
            Action::Frontier(action) => {
                if crate::frontier::requires_review(action) {
                    sandbox.last_message="Review the current named actor and destination before choosing this action.".into();None
                } else {
                    crate::frontier::act(action, &mut ui, &mut sandbox, &resident, &runtime, &mut control, &mut control_queue)
                }
            },
            Action::FrontierReviewed { action, token } => {
                if body_transition_pending(&commands,&control_queue) || net.as_ref().is_some_and(|net|!net.connected) {
                    sandbox.last_message="The active body or world is still changing. Review the named target after the transition before choosing this action.".into();None
                }else{
                    crate::frontier::act_reviewed(action, token, &mut ui, &mut sandbox, &resident, &runtime, &mut control, &mut control_queue)
                }
            },
            Action::Driving(action) => crate::driving::act(action, &mut ui, &mut sandbox, &mut control, &mut control_queue, scenes.as_deref()),
            Action::Fleet(action) => {
                if crate::fleet::requires_operation_review(action) {
                    sandbox.last_message="Review the displayed Fleet operation before choosing it.".into();None
                } else { crate::fleet::act(action, &mut ui, &mut sandbox) }
            },
            Action::FleetReviewed { action, token } => crate::fleet::act_reviewed_operation(action,token,&mut ui,&mut sandbox),
            Action::Community(action) => crate::communities::act(action, &mut ui, &mut sandbox),
            Action::ToggleWorld=>{ui.world_panel=!ui.world_panel;if ui.world_panel{ui.performance_panel=false;}None},
            Action::Diagnostics=>{ui.performance_panel=!ui.performance_panel;if ui.performance_panel{ui.world_panel=false;}None},
            Action::ResetDiagnostics=>{timings.reset();sandbox.last_message="Rolling performance samples reset.".into();None},
            Action::GraphicsQuality=>{quality.quality=quality.quality.next();sandbox.last_message=format!("{} graphics selected. Terrain detail updates in bounded jobs.",quality.quality.label());None},
            Action::Atmosphere=>{quality.atmospheric_sky=!quality.atmospheric_sky;None},
            Action::ImmersiveControls=>{ui.immersive_controls=!ui.immersive_controls;None},
            Action::ToggleInspector=>{ui.context_panel=!ui.context_panel;None},
            Action::Panel(panel)=>{ui.panel=panel;ui.context_panel=true;ui.panel_navigation_open=false;None},
            Action::TogglePanelNavigation=>{ui.panel_navigation_open=!ui.panel_navigation_open;None},
            Action::Mode(mode)=>{politics.atlas=false;placement.0=None;control_queue.0.push_back(ControlIntent::SetMode(mode));None},
            Action::Possess=>{politics.atlas=false;placement.0=None;control_queue.0.push_back(ControlIntent::PossessSelected);None},
            Action::Release=>{placement.0=None;control_queue.0.push_back(ControlIntent::Release);None},
            Action::SelectAll=>{control_queue.0.push_back(ControlIntent::SelectAll);None},
            Action::CycleActor=>{control_queue.0.push_back(ControlIntent::CycleActor);None},
            Action::SelectOwned(index)=>{if let Some(unit)=sandbox.state.living.units.values().filter(|u|u.owner==Some(player_id)).nth(index){control.selected.clear();control.selected.insert(unit.id);control.status=format!("Selected {}",unit.name);Some(Command::Living{action:LivingCommand::SelectActive{unit_id:unit.id}})}else{None}},
            Action::StopUnits|Action::RestUnits=>{
                let unit_ids:Vec<_>=crate::actor_summary::owned_selection(&sandbox,&control).take(terra_bevy::rules::MAX_GROUP_ORDER).collect();
                if unit_ids.is_empty(){sandbox.last_message="Select one or more owned actors before issuing this order.".into();None}
                else{Some(Command::Living{action:if matches!(action,Action::RestUnits){LivingCommand::Rest{unit_ids}}else{LivingCommand::StopUnits{unit_ids}}})}
            },
            Action::TradeQuantity=>{ui.trade_quantity=match ui.trade_quantity{1=>5,5=>10,_=>1};None},
            Action::NextMarket=>{crate::market::choose_next(&sandbox,&mut ui);None},
            Action::Buy(index)|Action::Sell(index)=>{
                match crate::market::trade_command(&sandbox,&ui,index,matches!(action,Action::Buy(_))) {
                    Ok(command)=>Some(command),
                    Err(error)=>{sandbox.last_message=error;None}
                }
            },
            Action::Eat|Action::Drink=>{sandbox.last_message="Review one selected active person before eating or drinking.".into();None},
            Action::ConsumeSelected{target,water}=>{
                if !target.matches(&sandbox,&control) || body_transition_pending(&commands,&control_queue) || net.as_ref().is_some_and(|net|!net.connected){
                    sandbox.last_message="The selected active body changed or is still changing. Review it again before eating or drinking.".into();None
                }else{Some(Command::Living{action:LivingCommand::Consume{item:if water{"water"}else{"bread"}.into()}})}
            },
            Action::GoToMarket=>{
                let target=crate::market::shop(&sandbox,&ui).map(|s|s.position);
                if let Some(destination)=target {
                    crate::frontier::move_to(&mut sandbox,destination,true,&resident,&runtime,&mut control,&mut control_queue)
                }else{sandbox.last_message="No actor and destination are available yet.".into();None}
            },
            Action::NextSite=>{let ids:Vec<_>=sandbox.state.living.producers.keys().copied().collect();if !ids.is_empty(){let current=ui.selected_site.or_else(||ids.first().copied());let next=ids.iter().position(|id|Some(*id)==current).map(|i|(i+1)%ids.len()).unwrap_or(0);ui.selected_site=Some(ids[next]);}None},
            Action::ToggleProduction|Action::GoToSite=>{sandbox.last_message="Review the displayed Industry facility and operation before choosing it.".into();None},
            Action::IndustryReviewed { token, travel } => {
                if let Some((site_id,destination,enabled))=crate::industry::reviewed_target(&mut sandbox,&mut ui,token,travel) {
                    if travel {crate::frontier::move_to(&mut sandbox,destination,true,&resident,&runtime,&mut control,&mut control_queue)}
                    else {Some(Command::Living{action:LivingCommand::SetProduction{site_id,enabled}})}
                } else {None}
            },
            Action::Preset(index)=>{politics.atlas=false;placement.0=None;control_queue.0.push_back(ControlIntent::Release);let presets=[(48.137,11.575,"Munich / Germany"),(47.421,10.985,"Zugspitze / Germany"),(53.541,9.985,"Hamburg / Germany"),(-33.9249,18.4241,"Cape Town / South Africa")];let(lat,lon,label)=presets[index];rig.teleport=GeoPosition::new(lat,lon,0.).ok();rig.altitude_offset=650.;ui.location=label.into();if let Some(net)=net.as_mut(){net.follow_player=false;}None},
            Action::EditGps=>{ui.input_mode=InputMode::Coordinates;ui.editing_gps=true;ui.gps_text.clear();None},
            Action::JumpGps=>{
                let value=ui.gps_text.trim().to_string();ui.editing_gps=false;
                match ui.input_mode {
                    InputMode::Coordinates=>{
                        let parts:Vec<_>=value.split(',').map(str::trim).collect();
                        let parsed=if parts.len()==2{parts[0].parse::<f64>().ok().zip(parts[1].parse::<f64>().ok()).and_then(|(lat,lon)|GeoPosition::new(lat,lon,0.).ok())}else{None};
                        if let Some(p)=parsed{politics.atlas=false;placement.0=None;control_queue.0.push_back(ControlIntent::Release);rig.teleport=Some(p);rig.altitude_offset=650.;ui.location="Custom geographic location".into();if let Some(net)=net.as_mut(){net.follow_player=false;}}
                        else{sandbox.last_message="Enter valid latitude, longitude, e.g. 48.137, 11.575".into();}None
                    },
                    InputMode::FactionName=>Some(Command::CreateFaction{name:value}),
                    InputMode::CompanyName=>Some(Command::Transport{action:terra_bevy::rules::TransportCommand::Company{action:terra_bevy::rules::CompanyCommand::Create{name:value.trim().into()}}}),
                    InputMode::DriverName=>Some(Command::Transport{action:terra_bevy::rules::TransportCommand::Company{action:terra_bevy::rules::CompanyCommand::HireDriver{name:value.trim().into()}}}),
                    InputMode::RouteName=>{if crate::launcher::model::validate_name(&value,32,"Route name").is_ok(){ui.fleet.route_name=value.trim().into();sandbox.last_message="Route name updated. Append stops, then create the route.".into();}else{sandbox.last_message="Choose a route name of 1–32 printable characters.".into();}None},
                    InputMode::SettlementName=>{
                        if ui.frontier.faction_claim && selected_faction.is_none() {
                            sandbox.last_message="Select a faction in Factions before founding a shared territory.".into();None
                        } else {
                            Some(Command::Settlement { action: terra_bevy::rules::SettlementCommand::Found { name:value, center:position, faction_owner:if ui.frontier.faction_claim {selected_faction}else{None} } })
                        }
                    },
                    mode=>crate::communities::finish_input(mode,&value,&mut ui,&mut sandbox),
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
            Action::Flour=>Some(Command::Craft{recipe:"flour".into(),count:1}),Action::Bread=>Some(Command::Craft{recipe:"bread".into(),count:1}),
            Action::Tool=>Some(Command::Craft{recipe:"tool".into(),count:1}),
            Action::Seeds=>Some(Command::Craft{recipe:"wheat_seed".into(),count:1}),
            Action::House|Action::Workbench|Action::Fence=>{politics.atlas=false;let blueprint=match action{Action::House=>"house",Action::Workbench=>"workbench",_=>"fence"};placement.0=Some(PlaceKind::Building(blueprint.into()));sandbox.last_message=format!("Place {blueprint}: click ground in your plot within 20 m of your player. Esc cancels.");None},
            Action::Plant=>{politics.atlas=false;placement.0=Some(PlaceKind::Crop);sandbox.last_message="Plant wheat: click free ground inside your plot, within 20 m. Esc cancels.".into();None},
            Action::Harvest=>sandbox.state.farms.values().filter(|f|sandbox.state.may_interact(player_id,f.faction_owner,f.owner)&&f.ready()).min_by(|a,b|a.position.surface_distance_m(position).total_cmp(&b.position.surface_distance_m(position))).map(|f|Command::Harvest{farm_id:f.id}).or_else(||{sandbox.last_message="No owned crop is ripe yet. Growth depends on time, temperature and moisture.".into();None}),
            Action::Damage|Action::ConfirmDamage=>{sandbox.last_message="Review the displayed named structure before damaging it.".into();None},
            Action::ReviewDamage(token)=>{crate::building_review::begin(token,&mut ui,&mut sandbox);None},
            Action::ConfirmDamageReviewed(token)=>crate::building_review::confirm(token,&mut ui,&mut sandbox),
            Action::CancelDamage=>{crate::building_review::cancel(&mut ui,&mut sandbox);None},
            Action::Home=>sandbox.state.structures.values().filter(|s|sandbox.state.may_use(player_id,s.faction_owner,s.owner)&&s.housing).min_by(|a,b|a.position.surface_distance_m(position).total_cmp(&b.position.surface_distance_m(position))).map(|s|Command::SetHome{structure_id:s.id}).or_else(||{sandbox.last_message="Build a house or join a faction with a shared house first.".into();None}),
            Action::CreateFaction|Action::InviteFaction|Action::PromoteFaction|Action::DemoteFaction=>{
                let mode=match action{Action::CreateFaction=>InputMode::FactionName,Action::InviteFaction=>InputMode::InvitePlayer,Action::PromoteFaction=>InputMode::PromotePlayer,_=>InputMode::DemotePlayer};crate::communities::start_input(mode,&mut ui,&mut sandbox);None
            },
            Action::AcceptFaction=>crate::communities::accept_invitation(&mut sandbox,&ui),
            Action::NextFaction=>{let ids:Vec<_>=sandbox.state.factions.values().filter(|f|f.members.contains_key(&player_id)).map(|f|f.id).collect();if !ids.is_empty(){let next=ids.iter().position(|id|Some(*id)==selected_faction).map(|i|(i+1)%ids.len()).unwrap_or(0);ui.selected_faction=Some(ids[next]);}crate::communities::faction_changed(&mut ui);None},
            Action::SharePlot=>{
                crate::communities::review_share_plot(&mut ui,&mut sandbox);None
            },
            Action::Atlas=>{politics.atlas=!politics.atlas;placement.0=None;None},
            Action::FitAtlas=>{sandbox.last_message=if politics.fit_at_epoch(runtime.clock.unix_seconds as i64){"Atlas framed to geometry valid at the simulation date.".into()}else{"No political geometry is valid at this date.".into()};None},
            Action::Borders=>{politics.borders=!politics.borders;None},
            Action::FlyToSelection=>{if let Some(p)=politics.selected{control_queue.0.push_back(ControlIntent::Release);rig.teleport=Some(p);rig.altitude_offset=650.;politics.atlas=false;ui.location="Selected atlas location".into();if let Some(net)=net.as_mut(){net.follow_player=false;}}else{sandbox.last_message="Click a location in the political atlas first.".into();}None},
            Action::DrawTerritory=>{if runtime.network_authority{sandbox.last_message="Draw scenario territories in a local session, export, then configure every peer with that dataset.".into();}else if politics.dataset.is_none(){sandbox.last_message="Load a political GeoJSON dataset before drawing territories.".into();}else{politics.atlas=true;politics.drawing=true;politics.draft.clear();placement.0=None;sandbox.last_message="Click territory vertices. Enter closes the boundary; Esc cancels. This is an authored scenario layer.".into();}None},
            Action::ExportPolitics=>{let path=runtime.config.save_path.with_extension("politics.geojson");sandbox.last_message=crate::output::check_managed(&runtime.config,options.as_ref().and_then(|options|options.world_profile_path.as_deref()),&path).and_then(|_|persistence.check_output_path(&path)).and_then(|_|politics.export(&path)).map(|_|format!("Exported {}",path.display())).unwrap_or_else(|e|format!("Export failed: {e}"));None},
            Action::Save=>{commands.0.push_back(SandboxIntent::Save);None},Action::Load=>{if let Some(flow) = flow.as_mut() { flow.request_restore(); queue.0.clear(); } else { sandbox.last_message="Restore review is unavailable in this presentation context.".into(); } None},
            Action::Follow=>{politics.atlas=false;placement.0=None;control_queue.0.push_back(ControlIntent::FocusSelection);None},
            Action::Overview=>{politics.atlas=false;control_queue.0.push_back(ControlIntent::Release);rig.position=LocalFrame::new(position).to_geo([200.,650.,650.]).unwrap_or(position);rig.yaw=-0.28;rig.pitch=-0.65;rig.needs_grounding=false;if let Some(net)=net.as_mut(){net.follow_player=false;}None},
        };
        if let Some(command) = command {
            commands.0.push_back(SandboxIntent::Command(command));
        }
    }
}

pub(crate) fn pick_ground(
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
    viewport: Res<WorldViewport>,
    flow: Option<Res<crate::flow::GameFlow>>,
    input: Res<crate::player_input::PlayerInput>,
    pointer: Res<crate::command_input::WorldPointer>,
) {
    if input.world_blocked() || politics.atlas || flow.as_ref().is_some_and(|f| f.blocks_input()) {
        return;
    }
    if keys.just_pressed(KeyCode::Escape) && flow.is_none() {
        placement.0 = None;
    }
    let Some(kind) = &placement.0 else {
        return;
    };
    let Some(cursor) = pointer.cursor(&window) else {
        return;
    };
    if !viewport.contains(&window, cursor, false) {
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
    let preview_command = match kind {
        PlaceKind::Building(blueprint) => Command::Build {
            blueprint: blueprint.clone(),
            position,
        },
        PlaceKind::Crop => Command::Plant {
            plot_id: plot.map(|p| p.id).unwrap_or(0),
            position,
            crop: "wheat".into(),
        },
    };
    let supported =
        terra_bevy::sandbox::placement_error(&preview_command, &sandbox, &runtime, &resident)
            .is_none();
    let valid = supported
        && plot.is_some()
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
    let confirm = if pointer.active {
        pointer.select_pressed
    } else {
        buttons.just_pressed(MouseButton::Left)
    };
    if confirm && valid {
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
    persistence: Res<terra_bevy::RegionalPersistence>,
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
    // Screenshot readiness still waits for the complete displayed neighborhood,
    // while loading uses the shared rule with its smaller startup cap.
    let expected = terra_bevy::living::startup_neighborhood(&runtime.config, runtime.focus);
    if !*captured
        && *frames >= 80
        && !expected.is_empty()
        && expected.iter().all(|key| resident.chunks.contains_key(key))
        && runtime.stream_stats.in_flight == 0
        && expected
            .iter()
            .all(|key| render.integrated_chunks.contains(key))
        && render.pending_meshes == 0
        && render.building_meshes == 0
    {
        if let Some(path) = &options.screenshot {
            let destination = std::path::Path::new(path);
            if let Err(error) = crate::output::check_managed(
                &runtime.config,
                options.world_profile_path.as_deref(),
                destination,
            )
            .and_then(|_| persistence.check_output_path(destination))
            {
                sandbox.last_message = format!("Screenshot destination rejected: {error}");
                eprintln!("Screenshot destination rejected: {error}");
                *failed = true;
                exit.write(AppExit::error());
                return;
            }
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(path.clone()));
            *captured = true;
        }
    }
    if options.exit_frames.is_some_and(|f| *frames >= f) {
        println!(
            "TERRA_RUN {}",
            serde_json::json!({"frames":*frames,"chunks":resident.chunks.len(),"rendered_chunks":render.chunks,"source_vertices":render.vertices,"pending_meshes":render.pending_meshes,"building_meshes":render.building_meshes,"population":sandbox.state.living.units.len(),"economy":sandbox.state.living.metrics,"rebases":runtime.rebases,"stream_poll_ms":runtime.stream_poll_ms,"sim_ms":runtime.simulation_ms})
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
    viewport: Res<WorldViewport>,
    flow: Option<Res<crate::flow::GameFlow>>,
    input: Res<crate::player_input::PlayerInput>,
) {
    if !politics.atlas
        || ui.editing_gps
        || input.world_blocked()
        || flow.as_ref().is_some_and(|f| f.blocks_input())
    {
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
    if !viewport.contains(&window, cursor, true) {
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
    use bevy::time::TimeUpdateStrategy;
    use std::{
        path::{Path, PathBuf},
        sync::{
            Arc,
            atomic::{AtomicU64, Ordering},
        },
        time::{Duration, Instant},
    };
    use terra_bevy::rules::Catalog;
    use terra_bevy::{RegionalPersistence, TerraWorldPlugin, WorldConfig};
    use terra_core::{ActorKind, HeightField, TileKey, WorldChunk};
    use terra_stream::{ChunkPayload, ChunkProvider, StreamError};

    static NEXT_SEED_FIXTURE: AtomicU64 = AtomicU64::new(1);

    #[test]
    fn atlas_draft_cannot_consume_management_or_suppressed_keys() {
        let mut app = seed_app(
            WorldConfig {
                time_scale: 0.,
                chunk_radius: 0,
                ..Default::default()
            },
            false,
        );
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<crate::player_input::PlayerInput>()
            .init_resource::<UiState>()
            .init_resource::<WorldViewport>()
            .add_systems(Update, atlas_interaction);
        app.world_mut().spawn(Window::default());
        app.world_mut().spawn((
            Camera::default(),
            GlobalTransform::default(),
            ExplorerCamera,
        ));
        let draft = vec![
            GeoPosition::new(48., 11., 0.).unwrap(),
            GeoPosition::new(48.1, 11., 0.).unwrap(),
            GeoPosition::new(48., 11.1, 0.).unwrap(),
        ];
        {
            let mut politics = app.world_mut().resource_mut::<PoliticalWorld>();
            politics.atlas = true;
            politics.drawing = true;
            politics.draft = draft.clone();
        }
        for key in [KeyCode::Enter, KeyCode::Backspace, KeyCode::Escape] {
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .press(key);
        }
        app.world_mut()
            .resource_mut::<crate::player_input::PlayerInput>()
            .ui_navigation = true;
        app.update();
        assert!(app.world().resource::<PoliticalWorld>().drawing);
        assert_eq!(app.world().resource::<PoliticalWorld>().draft, draft);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Backspace);
        app.world_mut()
            .resource_mut::<crate::player_input::PlayerInput>()
            .ui_navigation = false;
        app.update();
        assert_eq!(app.world().resource::<PoliticalWorld>().draft.len(), 2);
        app.world_mut()
            .resource_mut::<crate::player_input::PlayerInput>()
            .suppress = true;
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        app.update();
        assert!(app.world().resource::<PoliticalWorld>().drawing);
        assert_eq!(app.world().resource::<PoliticalWorld>().draft.len(), 2);
    }

    struct FlatSeedTerrain;
    impl ChunkProvider for FlatSeedTerrain {
        fn load(&self, key: TileKey) -> Result<ChunkPayload, StreamError> {
            Ok(ChunkPayload {
                cache_hit: false,
                chunk: WorldChunk {
                    key,
                    heights: HeightField::constant(100.),
                    features: vec![],
                    sources: vec![],
                    warnings: vec![],
                },
            })
        }
    }

    fn seed_directory(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "terra-demo-seed-{label}-{}-{}",
            std::process::id(),
            NEXT_SEED_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn seed_config(directory: &Path) -> WorldConfig {
        let mut focus = TileKey::at(GeoPosition::new(48.137, 11.575, 100.).unwrap(), 15)
            .unwrap()
            .center();
        focus.altitude_m = 300.25;
        WorldConfig {
            focus,
            chunk_radius: 0,
            max_resident_chunks: 1,
            max_queued: 1,
            max_in_flight: 1,
            terrain_resolution: 9,
            integrate_per_frame: 1,
            time_scale: 0.,
            live_weather: false,
            base_data_revision: "authored-demo-seed-fixture-v1".into(),
            cache_dir: directory.join("cache"),
            save_path: directory.join("legacy.json"),
            region_store: Some(directory.join("world.sqlite")),
            region_checkpoint_seconds: 3600.,
            ..Default::default()
        }
    }

    fn seed_app(config: WorldConfig, initialize_demo: bool) -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
                50,
            )))
            .insert_resource(DemoOptions::default())
            .add_plugins(TerraWorldPlugin::with_provider(
                config,
                Arc::new(FlatSeedTerrain),
            ));
        // Isolate demo initialization from environmental recovery and clock advancement.
        app.world_mut()
            .resource_mut::<SandboxRuntime>()
            .state
            .ecology
            .config
            .enabled = false;
        app.world_mut().resource_mut::<WorldRuntime>().clock.paused = true;
        if initialize_demo {
            install_seed_system(&mut app);
        }
        app
    }

    fn install_seed_system(app: &mut App) {
        app.add_systems(
            Update,
            seed_world.after(WorldSet::Stream).before(WorldSet::Sandbox),
        );
    }

    fn seed_loaded(app: &mut App, position: GeoPosition) {
        let key = TileKey::at(position, 15).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            app.update();
            assert!(
                !app.world().resource::<RegionalPersistence>().blocked(),
                "{}",
                app.world().resource::<SandboxRuntime>().last_message
            );
            let chunks = &app.world().resource::<ResidentChunks>().chunks;
            if chunks.len() == 1
                && chunks.contains_key(&key)
                && !app
                    .world()
                    .resource::<RegionalPersistence>()
                    .authority_paused()
            {
                break;
            }
            assert!(Instant::now() < deadline, "seed fixture terrain timed out");
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn seed_checkpoint(app: &mut App) {
        app.world_mut()
            .resource_mut::<SandboxQueue>()
            .0
            .push_back(SandboxIntent::Save);
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            app.update();
            let persistence = app.world().resource::<RegionalPersistence>();
            assert!(
                !persistence.blocked() || persistence.pending(),
                "{}",
                app.world().resource::<SandboxRuntime>().last_message
            );
            if app.world().resource::<SandboxQueue>().0.is_empty()
                && !persistence.authority_paused()
            {
                break;
            }
            assert!(Instant::now() < deadline, "seed checkpoint did not settle");
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn seed_export(app: &App) -> terra_bevy::sandbox::LocalWorldSave {
        let world = app.world();
        world
            .resource::<RegionalPersistence>()
            .export_world(
                world.resource::<SandboxRuntime>(),
                world.resource::<WorldRuntime>(),
                world.resource::<PoliticalWorld>(),
            )
            .unwrap()
    }

    #[test]
    fn regional_demo_seed_preserves_restored_airborne_actor_and_resource_history() {
        let directory = seed_directory("airborne-resume");
        let config = seed_config(&directory);
        let origin = config.focus;
        let far = GeoPosition::new(origin.latitude, origin.longitude + 1., 100.).unwrap();
        let airborne = GeoPosition {
            altitude_m: 850.125,
            ..origin
        };
        let mut game = seed_app(config.clone(), false);
        seed_loaded(&mut game, origin);
        let helicopter;
        {
            let mut sandbox = game.world_mut().resource_mut::<SandboxRuntime>();
            let SandboxRuntime { state, catalog, .. } = &mut *sandbox;
            state
                .seed_living_demo(
                    GeoPosition {
                        altitude_m: 100.025,
                        ..origin
                    },
                    &[],
                    catalog,
                )
                .unwrap();
            helicopter = state
                .living
                .units
                .values()
                .find(|unit| unit.owner == Some(1) && unit.kind == ActorKind::Helicopter)
                .unwrap()
                .id;
            state
                .apply(
                    1,
                    Command::Living {
                        action: LivingCommand::Possess {
                            unit_id: helicopter,
                        },
                    },
                    catalog,
                )
                .unwrap();
            state
                .set_controlled_motion(1, airborne, [2.5, 0., -1.25], 123.)
                .unwrap();
            state
                .seed_resource("authored:starter:wood".into(), far, "wood", 100, 45.)
                .unwrap();
            state
                .resource_nodes
                .get_mut("authored:starter:wood")
                .unwrap()
                .remaining = 7;
            // This saved world deliberately has no stone starter. Restore must
            // preserve both absence and the depleted, currently dormant wood.
        }
        seed_checkpoint(&mut game);
        assert!(game.world().resource::<SandboxRuntime>().state.is_partial());
        let expected = seed_export(&game);
        drop(game);

        let mut resumed = seed_app(config, true);
        seed_loaded(&mut resumed, airborne);
        for _ in 0..4 {
            resumed.update();
        }
        assert!(resumed.world().resource::<RegionalPersistence>().restored);
        let actual = seed_export(&resumed);
        assert_eq!(actual.state.players[&1].position, airborne);
        assert_eq!(actual.state.living.units[&helicopter].position, airborne);
        assert_eq!(
            actual.state.resource_nodes["authored:starter:wood"].remaining,
            7
        );
        assert!(
            !actual
                .state
                .resource_nodes
                .contains_key("authored:starter:stone")
        );
        assert_eq!(actual.state, expected.state);
        assert_eq!(actual.clock, expected.clock);
        assert_eq!(actual.player_id, expected.player_id);
        assert_eq!(actual.politics, expected.politics);
        drop(resumed);
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn regional_demo_seed_waits_while_blocked_and_checkpoints_valid_fresh_initialization() {
        let directory = seed_directory("blocked-fresh");
        let config = seed_config(&directory);
        let origin = config.focus;
        let mut game = seed_app(config, false);
        seed_loaded(&mut game, origin);
        game.world_mut()
            .resource_mut::<SandboxRuntime>()
            .state
            .players
            .get_mut(&1)
            .unwrap()
            .inventory
            .insert("invalid_game_item".into(), 1);
        game.world_mut()
            .resource_mut::<SandboxQueue>()
            .0
            .push_back(SandboxIntent::Save);
        let deadline = Instant::now() + Duration::from_secs(10);
        while !game.world().resource::<RegionalPersistence>().blocked() {
            game.update();
            assert!(
                Instant::now() < deadline,
                "failed seed checkpoint did not settle"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        install_seed_system(&mut game);
        let before = game.world().resource::<SandboxRuntime>().state.clone();
        let revision = game.world().resource::<SandboxRuntime>().revision;
        game.update();
        assert_eq!(game.world().resource::<SandboxRuntime>().state, before);
        assert_eq!(game.world().resource::<SandboxRuntime>().revision, revision);

        game.world_mut()
            .resource_mut::<SandboxRuntime>()
            .state
            .players
            .get_mut(&1)
            .unwrap()
            .inventory
            .remove("invalid_game_item");
        let generation = game
            .world()
            .resource::<RegionalPersistence>()
            .status
            .as_ref()
            .unwrap()
            .generation;
        seed_checkpoint(&mut game);
        game.update();
        let sandbox = game.world().resource::<SandboxRuntime>();
        assert_eq!(sandbox.state.players[&1].position.altitude_m, 100.);
        assert_eq!(sandbox.state.resource_nodes.len(), 2);
        for id in ["authored:starter:wood", "authored:starter:stone"] {
            assert_eq!(sandbox.state.resource_nodes[id].remaining, 100);
        }
        assert!(!sandbox.dirty);
        sandbox
            .state
            .validate_with_catalog(&sandbox.catalog)
            .unwrap();
        let persistence = game.world().resource::<RegionalPersistence>();
        assert!(!persistence.blocked());
        assert!(!persistence.restored);
        assert!(persistence.status.as_ref().unwrap().generation > generation);
        assert_eq!(persistence.last_checkpoint_reason, "Explicit checkpoint");
        assert!(game.world().resource::<WorldRuntime>().clock.paused);
        let seeded = seed_export(&game).state;
        for _ in 0..3 {
            game.update();
        }
        assert_eq!(seed_export(&game).state, seeded);
        drop(game);
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn regional_demo_seed_waits_for_source_hydration_before_checking_dormant_starter_ids() {
        let directory = seed_directory("dormant-fresh");
        let config = seed_config(&directory);
        let origin = config.focus;
        let far = GeoPosition::new(origin.latitude, origin.longitude + 1., 100.).unwrap();
        let mut game = seed_app(config, false);
        {
            let mut sandbox = game.world_mut().resource_mut::<SandboxRuntime>();
            sandbox
                .state
                .seed_resource("authored:starter:wood".into(), far, "wood", 100, 45.)
                .unwrap();
            sandbox
                .state
                .resource_nodes
                .get_mut("authored:starter:wood")
                .unwrap()
                .remaining = 7;
        }
        seed_loaded(&mut game, origin);
        assert!(!game.world().resource::<RegionalPersistence>().restored);
        let sandbox = game.world().resource::<SandboxRuntime>();
        assert!(sandbox.state.is_partial());
        assert!(
            !sandbox
                .state
                .resource_nodes
                .contains_key("authored:starter:wood")
        );
        let before = sandbox.state.clone();
        let revision = sandbox.revision;
        install_seed_system(&mut game);
        game.update();
        assert_eq!(game.world().resource::<SandboxRuntime>().state, before);
        assert_eq!(game.world().resource::<SandboxRuntime>().revision, revision);

        // A real source change supplies the complete overlay before the demo
        // initializer. The existing distant resource must win over new content.
        game.world_mut().resource_mut::<WorldRuntime>().focus = far;
        seed_loaded(&mut game, far);
        game.world_mut().resource_mut::<WorldRuntime>().focus = origin;
        seed_loaded(&mut game, origin);
        let state = seed_export(&game).state;
        assert_eq!(state.resource_nodes.len(), 2);
        assert_eq!(state.resource_nodes["authored:starter:wood"].position, far);
        assert_eq!(state.resource_nodes["authored:starter:wood"].remaining, 7);
        assert_eq!(
            state.resource_nodes["authored:starter:stone"].remaining,
            100
        );
        assert!(!game.world().resource::<SandboxRuntime>().dirty);
        assert_eq!(
            game.world()
                .resource::<RegionalPersistence>()
                .last_checkpoint_reason,
            "Explicit checkpoint"
        );
        drop(game);
        let _ = std::fs::remove_dir_all(directory);
    }

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

    #[test]
    fn quick_gather_skips_an_unplotted_protected_territory_at_its_boundary() {
        use terra_bevy::rules::SettlementCommand;

        let position = GeoPosition::new(48.137, 11.575, 100.).unwrap();
        let frame = LocalFrame::new(position);
        let center = frame.to_geo([101., 0., 0.]).unwrap();
        let forbidden = frame.to_geo([2., 0., 0.]).unwrap();
        let allowed = frame.to_geo([-9., 0., 0.]).unwrap();
        let catalog = Catalog::builtin();
        let mut state = SandboxState::default();
        state.add_player(1, position).unwrap();
        state.add_player(2, center).unwrap();
        state.seed_living_demo(position, &[], &catalog).unwrap();
        state.set_player_position(1, position).unwrap();
        state.set_player_position(2, center).unwrap();
        for (item, amount) in [("wood", 20), ("stone", 10)] {
            state
                .players
                .get_mut(&2)
                .unwrap()
                .inventory
                .insert(item.into(), amount);
        }
        state
            .apply(
                2,
                Command::Settlement {
                    action: SettlementCommand::Found {
                        name: "Protected grove".into(),
                        center,
                        faction_owner: None,
                    },
                },
                &catalog,
            )
            .unwrap();
        for (id, point) in [("near-protected", forbidden), ("clear-public", allowed)] {
            state
                .seed_resource(id.into(), point, "wood", 10, 0.)
                .unwrap();
        }
        assert!(
            state.plots.is_empty(),
            "territory rights must stand independently of plots"
        );
        assert!(state.check_settlement_interact(1, forbidden).is_err());
        let target = nearest_gatherable_node(&state, 1, position, "wood").unwrap();
        assert_eq!(target.id, "clear-public");
        let command = Command::Gather {
            node_id: target.id.clone(),
            amount: 5,
        };
        state.apply(1, command, &catalog).unwrap();
        assert_eq!(state.resource_nodes["near-protected"].remaining, 10);
        assert_eq!(state.resource_nodes["clear-public"].remaining, 5);
        state.validate_with_catalog(&catalog).unwrap();
    }
}
